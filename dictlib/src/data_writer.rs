use byteorder::{LittleEndian, WriteBytesExt};
use std::io::{BufWriter, Write};

pub struct DataWriter<T: std::io::Write> {
    pub write_len: usize,
    pub inner: BufWriter<T>,
}

impl DataWriter<std::fs::File> {
    pub fn new(path: &str) -> Self {
        let outfile = std::fs::File::create(path).unwrap();
        let inner = BufWriter::new(outfile);
        Self {
            write_len: 0,
            inner
        }
    }
}

impl<T: std::io::Write> DataWriter<T> {
    pub fn write_u8(&mut self, data: u8) -> std::io::Result<()> {
        self.write_len += 1;
        let _count = self.inner.write_u8(data)?;
        Ok(())
    }

    pub fn write_u16(&mut self, data: u16) -> std::io::Result<()> {
        self.write_len += 2;
        let _count = self.inner.write_u16::<LittleEndian>(data)?;
        Ok(())
    }

    pub fn write_u32(&mut self, data: u32) -> std::io::Result<()> {
        self.write_len += 4;
        let _count = self.inner.write_u32::<LittleEndian>(data)?;
        Ok(())
    }

    pub fn write_u64(&mut self, data: u64) -> std::io::Result<()> {
        self.write_len += 8;
        let _count = self.inner.write_u64::<LittleEndian>(data)?;
        Ok(())
    }

    pub fn write_f32(&mut self, data: f32) -> std::io::Result<()> {
        self.write_len += 4;
        let _count = self.inner.write_f32::<LittleEndian>(data)?;
        Ok(())
    }

    pub fn write_vbyte(&mut self, data: u64) -> std::io::Result<()> {
        let (length, encoded) = crate::vbyte::encode_vbyte(data);
        self.write_len += length as usize;

        let encoded_bytes = encoded.to_le_bytes();
        for i in 0..length {
            self.inner.write_u8(encoded_bytes[i as usize])?;
        }

        Ok(())
    }

    pub fn write_string(&mut self, str : &str) -> std::io::Result<()> {
        self.write_bytes_and_length(str.as_bytes())
    }

    pub fn write_bytes_and_length(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.write_u32(bytes.len() as u32)?;
        self.write_bytes(bytes)?;
        Ok(())
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.write_len += bytes.len() as usize;
        self.inner.write_all(bytes)?;
        Ok(())
    }

    pub fn write_utf8(&mut self, data: char) -> std::io::Result<()> {
        let count = data.len_utf8();

        let mut buffer : [u8;4] = [0;4];
        data.encode_utf8(&mut buffer);

        let _count = self.write_bytes(&buffer[0..count]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_vbyte() {
        let buffer = Vec::new();
        let buffer_writer = BufWriter::new(buffer);

        let mut writer = DataWriter {
            write_len: 0,
            inner: buffer_writer,
        };

        for i in 0..513 {
            writer.write_vbyte(i).unwrap();
        }

        for _i in 0..2 {
            // Add padding
            writer.write_u32(0).unwrap();
        }

        let mut reader = crate::data_reader::DataReader::new(writer.inner.buffer());

        for i in 0..513 {
            let read = reader.read_vbyte();
            assert_eq!(i as u64, read)
        }
    }
}
