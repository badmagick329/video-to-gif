//! The export slice's I/O: running ffmpeg, output files in the destination folder, the caption
//! image in the temp folder, the machine's free memory, the remembered destination, and the
//! clipboard.

pub mod captions;
pub mod clipboard;
pub mod destination;
pub mod job;
pub mod memory;
pub mod outputs;
