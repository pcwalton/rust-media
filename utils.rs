use std::io::{Error, ErrorKind, Result};
use streaming::StreamReader;

pub fn read_to_full(reader: &mut Box<Box<StreamReader>>, buf: &mut [u8]) -> Result<()> {
    let mut read = 0;
    loop {
        if read == buf.len() {
            return Ok(())
        }

        let bytes = try!(reader.read(&mut buf[read..]));

        if bytes == 0 {
            return Err(Error::new(ErrorKind::WriteZero, "found EOF where bytes expected"))
        }

        read += bytes;
    }
}
