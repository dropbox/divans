use core;
use codec::get_distance_from_mnemonic_code;
use alloc::{Allocator, SliceWrapperMut, SliceWrapper};
const CACHE_HIT_REFERENCE_SIZE: usize = 8;

pub struct CacheHitReferenceMut<'a>(pub &'a mut [u8]);


impl<'a> CacheHitReferenceMut<'a> {
    pub fn set_code_and_offset(&mut self, code: u8, mut offset: usize) {
        offset += 1;
        self.0[0] = code;
        self.0[1] = offset as u8;
        self.0[2] = (offset >> 8) as u8;
        self.0[3] = (offset >> 16) as u8;
        self.0[4] = (offset >> 24) as u8;
    }
}


pub struct CacheHitReference<'a>(pub &'a [u8]);


impl<'a> CacheHitReference<'a> {
    pub fn offset(&self) ->usize {
        (self.0[1] as usize | ((self.0[2] as usize) << 8)| ((self.0[3] as usize) << 16)| ((self.0[4] as usize) << 24)).wrapping_sub(1)
    }
    pub fn entry_id(&self) -> u8 {
        self.0[0]
    }
    pub fn miss(&self) -> bool {
        (self.0[1] | self.0[2] | self.0[3] | self.0[4] | self.0[5] | self.0[6] | self.0[7]) == 0
    }
}

#[derive(Debug,Copy,Clone)]
pub struct CacheEntry {
    dist:u32,
    origin_offset:usize,
}
pub struct Cache<AllocU8:Allocator<u8>> {
    cache:[CacheEntry;4],
    hitlist:AllocU8::AllocatedMemory,
}

impl<AllocU8:Allocator<u8>> Cache<AllocU8> {
    // prepares the cache statistics tracker for operating on num_commands
    pub fn new(cur_cache:&[u32;4], num_commands:usize, m8:&mut AllocU8) -> Self {
        Cache::<AllocU8>{
            cache:[CacheEntry{dist:cur_cache[0], origin_offset:0},
                   CacheEntry{dist:cur_cache[1], origin_offset:0},
                   CacheEntry{dist:cur_cache[2], origin_offset:0},
                   CacheEntry{dist:cur_cache[3], origin_offset:0}],
            hitlist:m8.alloc_cell(num_commands * 8),
        }
    }
    pub fn free(&mut self, m8:&mut AllocU8) {
        m8.free_cell(core::mem::replace(&mut self.hitlist, AllocU8::AllocatedMemory::default()));
    }
    pub fn get_cache_hit_log(&mut self, cmd_offset:usize) -> CacheHitReference{
        let mut index = cmd_offset * CACHE_HIT_REFERENCE_SIZE;
        if index + CACHE_HIT_REFERENCE_SIZE > self.hitlist.slice().len() { // if we somehow overestimated the cache size
            index = 0;
        }
        CacheHitReference(self.hitlist.slice_mut().split_at_mut(index).1)
    }
    fn get_cache_hit_log_mut(&mut self, cmd_offset:usize) -> CacheHitReferenceMut{
        let mut index = cmd_offset * CACHE_HIT_REFERENCE_SIZE;
        if index + CACHE_HIT_REFERENCE_SIZE > self.hitlist.len() { // if we somehow overestimated the cache size
            index = 0;
        }
        CacheHitReferenceMut(self.hitlist.slice_mut().split_at_mut(index).1)
    }
    fn forward_reference_hitlist(&mut self, code: u8, cache_index: u8, cmd_offset: usize, _dist:u32) {
        let origin = self.cache[usize::from(cache_index)].origin_offset;
        self.cache[usize::from(cache_index)].origin_offset = cmd_offset; // bump the "next use" of the cache
        let mut log = self.get_cache_hit_log_mut(origin);
        log.set_code_and_offset(code, cmd_offset);
    }
    pub fn populate(&mut self, dist:u32, copy_len:u32, cmd_offset:usize) {
        let cur_cache = [self.cache[0].dist, self.cache[1].dist, self.cache[2].dist, self.cache[3].dist];
        for code in 0..15 {
            let (cache_dist, ok, cache_index) = get_distance_from_mnemonic_code(&cur_cache, code as u8, copy_len);
            if dist == cache_dist && ok {
                // we have a hit
                self.forward_reference_hitlist(code, cache_index, cmd_offset, dist);
                break;
            }
        }
        let new_cache_entry = CacheEntry {
            dist:dist,
            origin_offset:cmd_offset,
        };
        // note the different logic here from the codec: we need to replace the cache entry, even if it's equal to 0 to get the right command index
        if dist == cur_cache[0] {
            self.cache[0] = new_cache_entry;
        } else if dist == cur_cache[1] {
            self.cache = [new_cache_entry, self.cache[0], self.cache[2], self.cache[3]];
        } else if dist == cur_cache[2] {
            self.cache = [new_cache_entry, self.cache[0], self.cache[1], self.cache[3]];           
        } else {
            self.cache = [new_cache_entry, self.cache[0], self.cache[1], self.cache[2]];
        }
    }
}

 
