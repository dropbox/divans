use super::interface::StructureSeeker;

#[derive(Default, Clone)]
pub struct Last8Parser {
    last8: u64,
}

impl StructureSeeker for Last8Parser {
    fn update(&mut self, data: &[u8]) {
        if data.len() >= 8 {
            let data_start = data.len() - 8;
            self.last8 = u64::from(data[data_start + 0])
                | (u64::from(data[data_start + 1]) << 0x8)
                | (u64::from(data[data_start + 2]) << 0x10)
                | (u64::from(data[data_start + 3]) << 0x18)
                | (u64::from(data[data_start + 4]) << 0x20)
                | (u64::from(data[data_start + 5]) << 0x28)
                | (u64::from(data[data_start + 6]) << 0x30)
                | (u64::from(data[data_start + 7]) << 0x38);
        } else {
            for item in data.iter() {
                self.update_literal(*item);
            }
        }
           
    }
    fn update_literal(&mut self, b:u8) {
        self.last8 >>= 8;
        self.last8 |= u64::from(b)<<0x38;
    }
    fn prior(&self) -> (u8, u8) {
        ((self.last8 >> 0x38) as u8, (self.last8 >> 0x30) as u8)
    }
}
