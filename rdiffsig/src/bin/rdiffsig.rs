extern crate rdiffsig;
extern crate alloc_no_stdlib as alloc;
use alloc::HeapAlloc;
use std::io::{Read, Write, Error, ErrorKind};
use std::env;
use std::fs::File;
use std::path::Path;
use rdiffsig::{
    FixedBuffer1,
    FixedBuffer2,
    FixedBuffer3,
    FixedBuffer4,
    FixedBuffer5,
    FixedBuffer6,
    FixedBuffer7,
    FixedBuffer8,
    FixedBuffer12,
    FixedBuffer16,
    FixedBuffer24,
    FixedBuffer32,
    Sig,
    SigFile,
    SigFileStat,
    CustomDictionary,
    CryptoSigTrait,
};

fn create_sig() {
    let mut base_file = Vec::<u8>::new();
    std::io::stdin().read_to_end(&mut base_file).unwrap();
    let mut m_fixed = alloc::HeapAlloc::<rdiffsig::Sig<FixedBuffer8>>::new(rdiffsig::Sig::<FixedBuffer8>::default());
    let sig = SigFile::<FixedBuffer8, alloc::HeapAlloc<rdiffsig::Sig<FixedBuffer8>>>::new(&mut m_fixed, 2048, &base_file[..]);
    //let _deserialized_sig = SigFile::<FixedBuffer8, alloc::HeapAlloc<rdiffsig::Sig<FixedBuffer8>>>::deserialize(&mut m_fixed, &sig_file[..]).unwrap();
    //let hint = sig.create_sig_hint();
    let mut buf = [0u8; 4096];
    let mut input_offset = 0usize;
    loop {
        let mut output_offset = 0usize;
        let done = sig.serialize(&mut input_offset, &mut buf, &mut output_offset);
        std::io::stdout().write_all(buf.split_at(output_offset).0).unwrap();
        if done {
            break;
        }
    }
}

fn specialized_process_input_file<R:Read,
                                  SigBuffer:CryptoSigTrait>(reader: &mut R,
                                                            sig: &[u8],
                                                            dict: &mut CustomDictionary<HeapAlloc<u8>>) -> Result<bool, Error> {
    let mut m_sig = HeapAlloc::<Sig<SigBuffer>>::new(Sig::<SigBuffer>::default());
    let mut sig_file = match SigFile::deserialize(&mut m_sig, sig) {
        Ok(sf) => sf,
        Err(_) => return Ok(false),
    };
    let hint = sig_file.create_sig_hint();
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf[..]) {
            Err(e) => {
                sig_file.free(&mut m_sig);
                return Err(e);
            },
            Ok(size) => {
                if size == 0 {
                    dict.flush(&hint, &sig_file);
                    break;
                } else {
                    dict.write(&buf[..size], &hint, &sig_file);
                }
            }
        }
    }
    sig_file.free(&mut m_sig);
    Ok(true)
}


fn process_input_file<R:Read>(reader: &mut R,
                              sig: &[u8],
                              dict:&mut CustomDictionary<HeapAlloc<u8>>) -> Result<(), Error> {
    match specialized_process_input_file::<R, FixedBuffer8>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer4>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer3>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer2>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer5>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer6>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer7>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer1>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer12>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer16>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer24>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    match specialized_process_input_file::<R, FixedBuffer32>(reader, sig, dict) {
        Err(e) => return Err(e),
        Ok(true) => return Ok(()),
        Ok(false) => {},
    }
    Err(Error::new(ErrorKind::InvalidInput, "Cannot parse .sig file"))
}

#[cfg(not(feature="create_sig"))]
fn main() {
    let mut sig_file = Vec::<u8>::new();
    let mut dict_file:Option<File> = None;
    let mut dict_mask_file: Option<File> = None;
    let mut input_file: Option<File> = None;
    let mut found_dict_file = false;
    let mut found_dict_mask_file = false;
    for argument in env::args().skip(1) {
        if argument == "-signature" {
            return create_sig();
        }
        if argument.starts_with("-dict=") {
            found_dict_file = true;
            continue;
        }
        if argument.starts_with("-dictmask=") {
            found_dict_mask_file = true;
            continue;
        }
        if argument.starts_with("-sig=") {
            let mut input = match File::open(&Path::new(&argument[5..])) {
                Err(why) => panic!("couldn't open {:}\n{:}", &argument[5..], why),
                Ok(file) => file,
            };
            input.read_to_end(&mut sig_file).unwrap();
            continue;
        }
        if input_file.is_some() {
            panic!("Exactly one argument expected for input file (extra: {:})", &argument);
        }
        input_file = Some(match File::open(&Path::new(&argument)) {
            Err(why) => panic!("couldn't open current block {:}\n{:}", &argument, why),
            Ok(file) => file,
        });
    }
    if found_dict_mask_file == false || found_dict_file == false {
        panic!("Must specify -dict= and -dictmask= to populate the dictionary into those files");
    }
    let sig_file_stat = SigFileStat::new(&sig_file[..]).unwrap();
    for argument in env::args().skip(1) {
        if argument.starts_with("-dict=") {
            dict_file = Some(match File::create(&Path::new(&argument[6..])) {
                Err(why) => panic!("couldn't open {:}\n{:}", &argument[6..], why),
                Ok(file) => file,
            });
            continue;
        }
        if argument.starts_with("-dictmask=") {
            dict_mask_file = Some(match File::create(&Path::new(&argument[10..])) {
                Err(why) => panic!("couldn't open {:}\n{:}", &argument[10..], why),
                Ok(file) => file,
            });
            continue;
        }
    }
    let mut m8 = alloc::HeapAlloc::<u8>::new(0);
    let mut custom_dict = CustomDictionary::<alloc::HeapAlloc<u8>>::new(
        &mut m8, sig_file_stat);
    if input_file.is_some() {
        process_input_file(&mut input_file.unwrap(), &sig_file[..], &mut custom_dict).unwrap();
    } else {
        process_input_file(&mut std::io::stdin(), &sig_file[..], &mut custom_dict).unwrap();
    }
    dict_file.unwrap().write_all(custom_dict.dict()).unwrap();
    dict_mask_file.unwrap().write_all(custom_dict.dict_mask()).unwrap();
    custom_dict.free(&mut m8);
}
