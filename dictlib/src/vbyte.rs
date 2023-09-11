use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Seek, SeekFrom};

const V_BYTE_LEN: [u8; 4] = [1, 2, 3, 5];
const V_BYTE_MASK: &[u64] = &[
    (1u64 << (1 * 8)) - 1,
    (1u64 << (2 * 8)) - 1,
    (1u64 << (3 * 8)) - 1,
    (1u64 << (5 * 8)) - 1,
];

pub fn read_vbyte(bs: &mut Cursor<&[u8]>) -> u64 {
    // Read full 8 bytes
    let value = match bs.read_u64::<LittleEndian>() {
        Ok(x) => x,
        Err(e) => {
            panic!(
                "Could not read v_byte value e={}, pos={}, buffer size={}",
                e,
                bs.position(),
                bs.get_ref().len()
            )
        }
    };

    let code = (value & 0x3) as usize;

    // The length of the stored data
    let length = V_BYTE_LEN[code] as i64;

    // Seek back to start + length of actual data
    let to_seek = -8 + length;
    if let Err(e) = bs.seek(SeekFrom::Current(to_seek)) {
        panic!("Could not seek {}, e={}", to_seek, e);
    }

    (value & V_BYTE_MASK[code]) >> 2
}

pub fn encode_vbyte(data: u64) -> (u8, u64) {
    let code = vbyte_len_code(data);
    let length = V_BYTE_LEN[code as usize];
    let encoded_value = (data << 2) | code as u64;
    (length, encoded_value)
}

fn vbyte_len_code(data: u64) -> u8 {
    const THRESHOLD_0: u64 = 1 << (V_BYTE_LEN[0] * 8 - 2);
    const THRESHOLD_1: u64 = 1 << (V_BYTE_LEN[1] * 8 - 2);
    const THRESHOLD_2: u64 = 1 << (V_BYTE_LEN[2] * 8 - 2);
    const THRESHOLD_3: u64 = 1 << (V_BYTE_LEN[3] * 8 - 2);

    if data >= THRESHOLD_3 {
        panic!("VByte too big!");
    }

    if (data < THRESHOLD_1) {
        if (data < THRESHOLD_0) {
            0
        } else {
            1
        }
    } else {
        if (data < THRESHOLD_2) {
            2
        } else {
            3
        }
    }
}
