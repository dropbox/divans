

pub struct CahcheHigReference<'a>(pub &'a [u8]);


impl<'a> CacheHitReference<'a> {
    pub fn offset(&self) ->usize {
        (self.0 & 0xffff_ffff_ffff) as usize
    }
    pub fn entry_id(&self) -> u8 {
        (self.0 >> 56) as u8
    }
    pub fn miss(&self) -> bool {
        self.entry_id() == 0xff
    }
}




pub struct CacheEntry {
    dist:u32,
    user_offset:u32,
    origin_offset:u32,
}
pub struct Cache<AllocU64> {
    cache:[CacheEntry;4],
    hitlist:Alloc
}
impl Default for Cache {
    fn default() -> Self {
        Cache([4,7,11,16])
    }
}

impl Cache {
    fn populate(&mut self, dist:u32, _copy_len:u32, command_id:usize) {
        
    }
}

 
