const CRC_MAGIC_16: u16 = 31;
const CRC_MAGIC: u32 = CRC_MAGIC_16 as u32;

pub fn crc_rollout(sum: u32, size: u32, old_byte_u8: u8) -> u32 {
    let size_16 = size as u16;
    let old_byte = u16::from(old_byte_u8);
    let mut s1 = (sum & 0xffff) as u16;
    let mut s2 = (sum >> 16) as u16;
    s1 = s1.wrapping_sub(old_byte.wrapping_add(CRC_MAGIC_16));
    s2 = s2.wrapping_sub(size_16.wrapping_mul(old_byte.wrapping_add(CRC_MAGIC_16)) as u16);
    u32::from(s1) | (u32::from(s2) << 16)
}

pub fn crc_rotate(sum: u32, size: u32, old_byte_u8: u8, new_byte_u8: u8) -> u32 {
    let size_16 = size as u16;
    let old_byte = u16::from(old_byte_u8);
    let new_byte = u16::from(new_byte_u8);
    let mut s1 = (sum & 0xffff) as u16;
    let mut s2 = (sum >> 16) as u16;
    s1 = s1.wrapping_add(new_byte.wrapping_sub(old_byte));
    s2 = s2.wrapping_add(s1.wrapping_sub(size_16.wrapping_mul(old_byte.wrapping_add(CRC_MAGIC_16))));
    u32::from(s1) | (u32::from(s2) << 16)
}

pub fn crc_update(sum: u32, buf: &[u8]) -> u32{
    let mut s1 = (sum & 0xffff) as u16;
    let mut s2 = (sum >> 16) as u16;
    for item in buf {
        s1 = s1.wrapping_add(u16::from(*item));
        s2 = s2.wrapping_add(s1);
    }
    let len = buf.len() as u32;
    s1 = s1.wrapping_add((len.wrapping_mul(CRC_MAGIC)) as u16);
    s2 = s2.wrapping_add((((len.wrapping_mul(len.wrapping_add(1))) / 2).wrapping_mul(CRC_MAGIC)) as u16);
    u32::from(s1) | (u32::from(s2) << 16)
}


#[cfg(test)]
mod test{
    use super::*;
    const BUFFER: &[u8] = b"This is a test of the emergency broadcast system. This is only a test";
    #[test]
    fn test_rotate() {
        let size = 16;
        let start = 16;
        let crc_16_32 = crc_update(0, &BUFFER[start..start + size]);
        let crc_0_16 = crc_update(0, &BUFFER[..size]);
        let mut roll_16_32 = crc_0_16;
        for i in 0..start {
            roll_16_32 = crc_rotate(roll_16_32, size as u32, BUFFER[i], BUFFER[i + size]);
        }
        assert_eq!(roll_16_32, crc_16_32);
        assert_eq!(roll_16_32, 0x40d707b6);
    }
    #[test]
    fn test_rollout() {
        let size = 16;
        for variant in 0..size {
            let start = BUFFER.len() - size + variant;
            let crc_end = crc_update(0, &BUFFER[start..]);
            let crc_0_16 = crc_update(0, &BUFFER[..size]);
            let mut roll_end = crc_0_16;
            for i in 0..(BUFFER.len() - size) {
                roll_end = crc_rotate(roll_end, size as u32, BUFFER[i], BUFFER[i + size]);
            }
            let mut shrinking_size = size;
            for i in (BUFFER.len() - size)..start {
                roll_end = crc_rollout(roll_end, shrinking_size as u32, BUFFER[i]);
                shrinking_size -= 1;
            }
            assert_eq!(roll_end, crc_end);
            if variant == 3 { // make sure at least one of them complies with the predetermined value
                assert_eq!(roll_end, 0x2b200649);
            }
        }
    }
}
