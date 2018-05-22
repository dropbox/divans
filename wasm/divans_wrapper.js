var wasm_divans_bytecode = '../target/wasm32-unknown-unknown/release/divans.wasm';
var options_map;
var desired_option_list = [];
var BUFFER_SIZE = 16384;
var max_quality = 11;
function set_max_quality(q) {
   max_quality = q;
}
function makeQualityDialog() {
    var quality_dialog = document.createElement("select");
    quality_dialog.style.float="right"
    values = ["11", "10", "9.5", "9", "8", "7", "6", "5", "4", "3", "2"]
    for (var index = 0; index < values.length; index+=1) {
       var opt = document.createElement("option");
       opt.value = values[index];
       opt.appendChild(document.createTextNode("Quality:" + values[index]));
       quality_dialog.appendChild(opt);
    }
    return quality_dialog;
}
function is_divans(file) {
    return file.length > 16 && file[0] == 255 && file[1] == 229 && file[2] == 140 && file[3] == 159;
}
function encode(external_uint8_buffer, obj_instance_exports, debug, encoder_options) {
    var list_of_encoded_buffers = [];
    var desired_quality = encoder_options['quality'];
    if (!desired_quality) {
       encoder_options['quality'] = max_quality | 0;
       if (max_quality > 9 && max_quality < 10)  {
           encoder_options['quality'] = 11;
           encoder_options['q9_5'] = 1;
       }
    } else {
        if (desired_quality > max_quality) {
            if (max_quality > 9 && max_quality < 10)  {
               encoder_options['quality'] = 11;
               encoder_options['q9_5'] = 1;
            } else {
               encoder_options['quality'] = max_quality | 0;
            }
        }
    }
    var divans = obj_instance_exports;
    var state = divans.divans_new_compressor();
    for (key in encoder_options) {
       var value = encoder_options[key];
       var c_option_id = options_map[key]
       if (c_option_id !== undefined) {
          var ret = divans.divans_set_option(state, c_option_id, value);
          if (ret != 0) {
             console.log("Tried to set option " + key + " (" + c_option_id+ ") to " + value + " failed: " + ret);
          }
       } else {
          console.log("Unknown encoder option " + key + " : " + value);
       }
    }
    var input_ptr = divans.divans_compressor_malloc_u8(state, external_uint8_buffer.length);
    var output_ptr = divans.divans_compressor_malloc_u8(state, BUFFER_SIZE);
    var input_offset_ptr = divans.divans_compressor_malloc_usize(state, 1);
    var output_offset_ptr = divans.divans_compressor_malloc_usize(state, 1);
    var input_buf = new Uint8Array(divans.memory.buffer, input_ptr, external_uint8_buffer.length);
    var output_buf = new Uint8Array(divans.memory.buffer, output_ptr, BUFFER_SIZE);
    var input_offset_buf = new Uint32Array(divans.memory.buffer, input_offset_ptr, 1);
    input_buf.set(external_uint8_buffer)
    var output_size = 0;
    var ret = 0;
    do {
       ret = divans.divans_encode(state, input_ptr, external_uint8_buffer.length, input_offset_ptr,
                      output_ptr, BUFFER_SIZE, output_offset_ptr);

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
       var input_offset_buf = new Uint32Array(divans.memory.buffer,input_offset_ptr, 1);
       if (input_offset_buf[0] == external_uint8_buffer.length) {
         break;
       }
       if (debug) {
           console.log("working:" + input_offset_buf[0] + "/" + external_uint8_buffer.length);
       }
    } while(ret != 0 && ret != 3);
    do {
       ret = divans.divans_encode_flush(state,
                      output_ptr, BUFFER_SIZE, output_offset_ptr);
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
    divans.divans_compressor_free_u8(state, output_ptr, BUFFER_SIZE);
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
    var output_ptr = divans.divans_decompressor_malloc_u8(state, BUFFER_SIZE);
    var input_offset_ptr = divans.divans_decompressor_malloc_usize(state, 1);
    var output_offset_ptr = divans.divans_decompressor_malloc_usize(state, 1);
    var input_buf = new Uint8Array(divans.memory.buffer, input_ptr, external_uint8_buffer.length);
    var output_buf = new Uint8Array(divans.memory.buffer, output_ptr, BUFFER_SIZE);
    var input_offset_buf = new Uint32Array(divans.memory.buffer, input_offset_ptr, 1);
    input_buf.set(external_uint8_buffer)
    var output_size = 0;
    var ret = 0;
    do {
       ret = divans.divans_decode(state, input_ptr, external_uint8_buffer.length, input_offset_ptr,
                      output_ptr, BUFFER_SIZE, output_offset_ptr);
       var input_offset_buf = new Uint32Array(divans.memory.buffer,input_offset_ptr, 1);
       if (debug) {
            console.log(ret);
       }
       var output_offset_buf = new Uint32Array(divans.memory.buffer,output_offset_ptr, 1);
       if (output_offset_buf[0] !== 0) {
            output_size += output_offset_buf[0];
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
       if (ret == 1  & input_offset_buf[0] !=  external_uint8_buffer.length) {
          break; // we need more input, but we have no more input to give
       }
    } while(ret != 0 && ret != 3);
    if (debug) {
        console.log("TOTAL SIZE " + output_size);
    }
    divans.divans_decompressor_free_u8(state, input_ptr, external_uint8_buffer.length);
    divans.divans_decompressor_free_u8(state, output_ptr, BUFFER_SIZE);
    divans.divans_decompressor_free_usize(state, input_offset_ptr, 1);
    divans.divans_decompressor_free_usize(state, output_offset_ptr, 1);
    divans.divans_free_decompressor(state);
    if (ret != 0) {
        return [];
    }
    return list_of_decoded_buffers;
}
var self = this;
if (self.document === undefined) {
    var msg_buffer = []
    self.onmessage = function(e) {
        msg_buffer[msg_buffer.length] = e;
    };
    const memory = new WebAssembly.Memory({ initial: 256, maximum: 4096 });
    const importObj = {
      env: {
          log2f: Math.log2,
          log2: Math.log2,
          exp2f: function(a) {return Math.exp(2, a);},
          exp2: function(a) {return Math.exp(2, a);},
          abortStackOverflow: () => { throw new Error('overflow'); },
          table: new WebAssembly.Table({ initial: 0, maximum: 0, element: 'anyfunc' }),
          tableBase: 0,
          memory: memory,
          memoryBase: 1024,
          STACKTOP: 0,
          STACK_MAX: memory.buffer.byteLength
      }
   };
   function wasm_callback(object) {
       function thread_process(event) {
          var jid = event.data[0];
          var buffer = event.data[1];
          var option_id = event.data[2];
          max_quality = event.data[3];
          var result;
          if (is_divans(buffer)) {
             result = decode(buffer, object.instance.exports, false);
          } else {
             let new_options = {}
             for (key in desired_option_list[option_id]) {
                new_options[key] = desired_option_list[option_id][key];
             }
             result = encode(buffer, object.instance.exports, false, new_options);
          }
          postMessage([jid, result]);
       }
       self.onmessage = function(e) {
          thread_process(e);
       }
       for (var i = 0; i < msg_buffer.length; i+= 1) {
          thread_process(msg_buffer[i])
       }
   }
   if (WebAssembly.instantiateStreaming) {
       WebAssembly.instantiateStreaming(fetch(wasm_divans_bytecode),
          importObj).then(function (web_obj) {
              /*for (var i = 0; i < workers.length; i +=1) { // doesnt work in chrome
                 workers[i].postMessage(web_obj.module);
              }*/
              return wasm_callback(web_obj);
            })/*.catch((function(reason){
            console.error("wasm streaming compile failed: "+reason);
       }))*/;
    } else {
         var gmodule = null;
         fetch(wasm_divans_bytecode).then(response =>
         response.arrayBuffer()).then(bytes =>
             WebAssembly.compile(bytes)).then(function(module) {
                
                gmodule = module;
                /*for (var i = 0; i < workers.length; i +=1) { // nonfunctional in chrome
                   workers[i].postMessage(module);
                }*/
                return WebAssembly.instantiate(module, importObj);
             }).then(instance => wasm_callback({instance:instance, module:gmodule}));
    }    
}
options_map = { // taken from ffi.h
   "quality": 1,
   "window_size": 2,
   "lgblock":3,
   "dynamic_context_mixing":4,
   "use_brotli_command_selection": 5,
   "use_brotli_bitstream": 6,
   "use_context_map": 7,
   "literal_adaptation_cm_high": 8,
   "literal_adaptation": 8,
   "force_stride_value": 9,
   "stride_detection_quality": 10,
   "prior_depth": 11,
   "literal_adaptation_stride_high": 12,
   "literal_adaptation_cm_low": 13,
   "literal_adaptation_stride_low": 14,
   "brotli_literal_byte_score": 15,
   "speed_detection_quality": 16,
   "prior_bitmask_detection": 17,
   "q9_5": 18,
   "force_literal_context_mode": 19,
};
desired_option_list[desired_option_list.length] = {
   "quality": 2,
   "window_size": 22,
   "force_literal_context_mode": 0, //  lsb
   "literal_adaptation": 0, // just serialize the bits
   "brotli_literal_byte_score": 840,
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "window_size": 22,
   "force_literal_context_mode": 0, // lsb
   "literal_adaptation": 8, // 16,8192
   "brotli_literal_byte_score": 440, // ignored
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "q9_5": 1,
   "window_size": 22,
   "force_literal_context_mode": 3, // sign
   "literal_adaptation": 9, // 32,4096
   "brotli_literal_byte_score": 140,
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "q9_5": 1,
   "window_size": 22,
   "force_literal_context_mode": 3, // sign
   "literal_adaptation": 8, // 16,8192
   "brotli_literal_byte_score": 40,
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "window_size": 18,
   "literal_adaptation": 8,
   "brotli_literal_byte_score": 840, // ignored
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "q9_5": 1,
   "window_size": 22,
   "force_literal_context_mode": 0,
   "literal_adaptation": 1, // 2,1024
   "brotli_literal_byte_score": 340,
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "window_size": 22,
   "force_literal_context_mode": 0, //  lsb
   "literal_adaptation": 3, // 1,16384
   "brotli_literal_byte_score": 540, // ignored
};
desired_option_list[desired_option_list.length] = {
   "quality": 11,
   "q9_5": 1,
   "window_size": 22,
   "force_literal_context_mode": 0, //  lsb
   "literal_adaptation": 8, // 1,16384
   "brotli_literal_byte_score": 40,
};
