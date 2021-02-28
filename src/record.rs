use rusqlite::{self, params, Connection};

use crate::udp::{FromUdp, ToUdp};

#[derive(Debug, PartialEq)]
/// Some dummy data.
pub struct Record {
    pub id: u32,
    pub data: String,
}

#[derive(Debug, PartialEq)]
/// Represents errors that can occur while
/// parsing [Record] from bytes.
pub enum ParseError {
    /// Got less that 4 bytes.
    Incomplete(usize),
    /// Failed to parse UTF-8 string.
    Invalid(std::string::FromUtf8Error),
}

impl Record {
    pub fn load(conn: Connection) -> rusqlite::Result<Vec<Self>> {
        let mut query = conn.prepare("SELECT id, data FROM records")?;
        let records = query.query_map(params![], |row| {
            Ok(Record {
                id: row.get(0)?,
                data: row.get(1)?,
            })
        })?;
        records.collect()
    }
}

impl FromUdp for Record {
    type Error = ParseError;

    fn from_udp(buf: &[u8]) -> Result<Self, Self::Error> {
        if buf.len() < 4 {
            return Err(ParseError::Incomplete(buf.len()));
        }

        let mut id = [0_u8; 4];
        id.copy_from_slice(&buf[..4]);
        let id = u32::from_le_bytes(id);

        Ok(Self {
            id,
            data: String::from_utf8(buf[4..].to_vec()).map_err(|e| ParseError::Invalid(e))?,
        })
    }
}

impl ToUdp for Record {
    fn to_udp(&self) -> Vec<u8> {
        let id_bytes = self.id.to_le_bytes();
        let str_bytes = self.data.as_bytes();
        [&id_bytes, str_bytes].concat()
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};

    use crate::record::{ParseError, Record};
    use crate::udp::FromUdp;

    #[test]
    fn udp_incomplete() {
        assert_eq!(Record::from_udp(&[0, 0]), Err(ParseError::Incomplete(2)))
    }

    #[test]
    fn udp() {
        assert_eq!(
            Record::from_udp(&[1, 0, 0, 0, 'r' as u8]),
            Ok(Record {
                id: 1,
                data: "r".to_owned()
            })
        )
    }

    #[test]
    fn udp_non_utf() {
        match Record::from_udp(&[1, 0, 0, 0, 0xc3, 0x28]) {
            Err(ParseError::Invalid(_)) => {}
            Err(e) => {
                panic!(e)
            }
            Ok(record) => {
                panic!(
                    "Incorrectly parsed record from invalid utf-8: {:#?}",
                    record
                )
            }
        }
    }

    #[test]
    fn load() {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE records (
                       id INTEGER PRIMARY KEY,
                       data TEXT NOT NULL
                )",
            params![],
        )
        .unwrap();

        let mut records: Vec<Record> = vec![
            (0, "Record"),
            (u32::MAX, "other"),
            (42, "ᚻᛖ ᚳᚹᚫᚦ"),
            (256, "░░▒▒▓▓██"),
            (1732454, "HTML tags lea͠ki̧n͘g fr̶ǫm ̡yo​͟ur eye͢s̸ ̛l̕ik͏e liq​uid pain"),
        ]
        .into_iter()
        .map(|(id, data)| {
            let data = data.to_owned();
            conn.execute("INSERT INTO records VALUES (?1, ?2)", params![&id, &data])
                .unwrap();
            Record { id, data }
        })
        .collect();
        records.sort_by_key(|r| r.id);
        let mut loaded = Record::load(conn).unwrap();
        loaded.sort_by_key(|r| r.id);

        assert_eq!(loaded, records);
    }

    #[test]
    fn load_no_table() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(Record::load(conn).is_err());
    }

    #[test]
    fn load_no_fields() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE records (
                       id INTEGER PRIMARY KEY
                )",
            params![],
        )
        .unwrap();
        assert!(Record::load(conn).is_err());
    }
}
