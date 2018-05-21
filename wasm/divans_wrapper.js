function is_divans(file) {
    return file.length > 16 && file[0] == 255 && file[1] == 229 && file[2] == 140 && file[3] == 159;
}
function encode(external_uint8_buffer, obj_instance_exports, debug) {
    var list_of_encoded_buffers = [];
    var divans = obj_instance_exports;
    var state = divans.divans_new_compressor();
    var input_ptr = divans.divans_compressor_malloc_u8(state, external_uint8_buffer.length);
    var output_ptr = divans.divans_compressor_malloc_u8(state, 4096);
    var input_offset_ptr = divans.divans_compressor_malloc_usize(state, 1);
    var output_offset_ptr = divans.divans_compressor_malloc_usize(state, 1);
    var input_buf = new Uint8Array(divans.memory.buffer, input_ptr, external_uint8_buffer.length);
    var output_buf = new Uint8Array(divans.memory.buffer, output_ptr, 4096);
    var input_offset_buf = new Uint32Array(divans.memory.buffer, input_offset_ptr, 1);
    input_buf.set(external_uint8_buffer)
    var output_size = 0;
    var ret = 0;
    do {
       ret = divans.divans_encode(state, input_ptr, external_uint8_buffer.length, input_offset_ptr,
                      output_ptr, 4096, output_offset_ptr);
       var output_offset_buf = new Uint32Array(divans.memory.buffer,output_offset_ptr, 1);
       output_size += output_offset_buf[0];
       if (debug) {
           console.log(ret);
       }
       var output_offset_buf = new Uint32Array(divans.memory.buffer,output_offset_ptr, 1);
       if (output_offset_buf[0] !== 0) {
            if (debug) {
                 console.log(output_offset_buf[0]);
            }
            var produced_data = new Uint8Array(divans.memory.buffer, output_ptr, output_offset_buf[0]);
            if (debug) {
                 console.log(produced_data);
            }
            list_of_encoded_buffers[list_of_encoded_buffers.length] = new Uint8Array(produced_data);
       }
       output_offset_buf[0] = 0;
    } while(ret != 0 && ret != 1);
    do {
       ret = divans.divans_encode_flush(state,
                      output_ptr, 4096, output_offset_ptr);
       var output_offset_buf = new Uint32Array(divans.memory.buffer,output_offset_ptr, 1);
       output_size += output_offset_buf[0];
       if (output_offset_buf[0] != 0) {
            var produced_data = new Uint8Array(divans.memory.buffer, output_ptr, output_offset_buf[0]);
            if (debug) {
                console.log(produced_data);
            }
            list_of_encoded_buffers[list_of_encoded_buffers.length] = new Uint8Array(produced_data);
       }
       output_offset_buf[0] = 0;
    } while(ret != 0);
    if (debug) {
        console.log("TOTAL SIZE " + output_size);
    }
    divans.divans_compressor_free_u8(state, input_ptr, external_uint8_buffer.length);
    divans.divans_compressor_free_u8(state, output_ptr, 4096);
    divans.divans_compressor_free_usize(state, input_offset_ptr, 1);
    divans.divans_compressor_free_usize(state, output_offset_ptr, 1);
    divans.divans_free_compressor(state);
    return list_of_encoded_buffers;
}
function decode(external_uint8_buffer, obj_instance_exports, debug) {
    var list_of_decoded_buffers = [];
    var divans = obj_instance_exports;
    var state = divans.divans_new_serial_decompressor();
    var input_ptr = divans.divans_decompressor_malloc_u8(state, external_uint8_buffer.length);
    var output_ptr = divans.divans_decompressor_malloc_u8(state, 4096);
    var input_offset_ptr = divans.divans_decompressor_malloc_usize(state, 1);
    var output_offset_ptr = divans.divans_decompressor_malloc_usize(state, 1);
    var input_buf = new Uint8Array(divans.memory.buffer, input_ptr, external_uint8_buffer.length);
    var output_buf = new Uint8Array(divans.memory.buffer, output_ptr, 4096);
    var input_offset_buf = new Uint32Array(divans.memory.buffer, input_offset_ptr, 1);
    input_buf.set(external_uint8_buffer)
    var output_size = 0;
    var ret = 0;
    do {
       ret = divans.divans_decode(state, input_ptr, external_uint8_buffer.length, input_offset_ptr,
                      output_ptr, 4096, output_offset_ptr);
       var output_offset_buf = new Uint32Array(divans.memory.buffer,output_offset_ptr, 1);
       output_size += output_offset_buf[0];
       if (debug) {
            console.log(ret);
       }
       var output_offset_buf = new Uint32Array(divans.memory.buffer,output_offset_ptr, 1);
       if (output_offset_buf[0] !== 0) {
            if (debug) {
                console.log(output_offset_buf[0]);
            }
            var produced_data = new Uint8Array(divans.memory.buffer, output_ptr, output_offset_buf[0]);
            if (debug) {
                console.log(produced_data);
            }
            list_of_decoded_buffers[list_of_decoded_buffers.length] = new Uint8Array(produced_data);
       }
       output_offset_buf[0] = 0;
    } while(ret != 0 && ret != 1 && ret != 3);
    if (debug) {
        console.log("TOTAL SIZE " + output_size);
    }
    divans.divans_decompressor_free_u8(state, input_ptr, external_uint8_buffer.length);
    divans.divans_decompressor_free_u8(state, output_ptr, 4096);
    divans.divans_decompressor_free_usize(state, input_offset_ptr, 1);
    divans.divans_decompressor_free_usize(state, output_offset_ptr, 1);
    divans.divans_free_decompressor(state);
    if (ret != 0) {
        return [];
    }
    return list_of_decoded_buffers;
}
