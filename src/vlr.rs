use byteorder::{LittleEndian, ReadBytesExt};
use std::fmt;
use std::io::Read;

/// LAS Variable Length Records
pub struct Vlr {
    user_id: [u8; 16],
    pub(crate) record_id: u16,
    description: [u8; 32],
    pub(crate) data: Vec<u8>,
}

impl Vlr {
    pub fn read_from<R: Read>(src: &mut R) -> std::io::Result<Self> {
        src.read_u16::<LittleEndian>()?; // reserved
        let mut user_id = [0u8; 16];
        src.read_exact(&mut user_id)?;

        let record_id = src.read_u16::<LittleEndian>()?;
        let record_length = src.read_u16::<LittleEndian>()?;

        let mut description = [0u8; 32];
        src.read_exact(&mut description)?;

        let mut data = Vec::<u8>::new();
        data.resize(record_length as usize, 0);
        src.read_exact(&mut data)?;

        Ok(Self {
            user_id,
            record_id,
            description,
            data,
        })
    }
    pub fn user_id(&self) -> String {
        String::from_utf8_lossy(&self.user_id)
            .trim_end_matches(|c| c as u8 == 0)
            .to_string()
    }
    pub fn description(&self) -> String {
        String::from_utf8_lossy(&self.description)
            .trim_end_matches(|c| c as u8 == 0)
            .to_string()
    }
}

impl fmt::Debug for Vlr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vlr")
            .field("user_id", &self.user_id())
            .field("record_id", &self.record_id)
            .field("description", &self.description())
            .field("data", &format_args!("[u8; {}]", self.data.len()))
            .finish()
    }
}
