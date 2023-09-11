use byteorder::{LittleEndian, ReadBytesExt};

pub struct DataReader<'a> {
    pub data: &'a [u8],
    pub position: usize,
}

impl<'a> DataReader<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    #[inline]
    pub fn new_at(data: &'a [u8], position: usize) -> Self {
        Self { data, position }
    }

    #[inline]
    pub fn read_u8(&mut self) -> u8 {
        let mut cursor = std::io::Cursor::new(&self.data[0..]);
        cursor.set_position(self.position as u64);
        let data = cursor.read_u8().unwrap();
        self.position += std::mem::size_of::<u8>();
        data
    }

    #[inline]
    pub fn read_u16(&mut self) -> u16 {
        let mut cursor = std::io::Cursor::new(&self.data[0..]);
        cursor.set_position(self.position as u64);
        let data = cursor.read_u16::<LittleEndian>().unwrap();
        self.position += std::mem::size_of::<u16>();
        data
    }

    #[inline]
    pub fn read_u32(&mut self) -> u32 {
        let mut cursor = std::io::Cursor::new(&self.data[0..]);
        cursor.set_position(self.position as u64);
        let data = cursor.read_u32::<LittleEndian>().unwrap();
        self.position += std::mem::size_of::<u32>();
        data
    }

    #[inline]
    pub fn read_u64(&mut self) -> u64 {
        let mut cursor = std::io::Cursor::new(&self.data[0..]);
        cursor.set_position(self.position as u64);
        let data = cursor.read_u64::<LittleEndian>().unwrap();
        self.position += std::mem::size_of::<u64>();
        data
    }

    #[inline]
    pub fn read_bytes_len(&mut self, len: usize) -> &'a [u8] {
        let slice = &self.data[self.position..self.position + len];
        self.position += len;
        slice
    }

    #[inline]
    pub fn read_utf8_char(&mut self) -> char {
        let slice = &self.data[self.position..self.position + 4];
        let s = unsafe { std::str::from_utf8_unchecked(slice) };
        let c = s.chars().next().unwrap();

        self.position += c.len_utf8();

        c
    }

    #[inline]
    pub fn read_string(&mut self) -> &'a str {
        let len = self.read_u32();
        let bytes = self.read_bytes_len(len as usize);

        std::str::from_utf8(bytes).unwrap()
    }

    #[inline]
    pub fn read_vbyte(&mut self) -> u64 {
        let mut cursor = std::io::Cursor::new(&self.data[0..]);
        cursor.set_position(self.position as u64);
        let value = crate::vbyte::read_vbyte(&mut cursor);
        self.position = cursor.position() as usize;
        value
    }

    #[inline]
    pub fn peek_vbyte(&self) -> u64 {
        let mut cursor = std::io::Cursor::new(&self.data[0..]);
        cursor.set_position(self.position as u64);
        crate::vbyte::read_vbyte(&mut cursor)
    }

    #[inline]
    pub fn read_offset_string(&mut self) -> crate::OffsetString {
        let len = self.read_u32();

        let offset = crate::OffsetString {
            start: self.position as u32,
            len,
        };

        self.position += len as usize;

        offset
    }

    #[inline]
    pub fn skip(&mut self, offset: usize) {
        self.position += offset;
    }
}
