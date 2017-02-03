use common::MapIterator;
use common::num::Cast;
use file_offset::FileExt;
use std::error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io;
use std::ops;
use std::path::Path;

use format::ItemView;
use format;
use raw::CallbackNew;
use raw::CallbackReadData;
use raw::DataCallback;
use raw::ResultExt;
use raw;

#[derive(Debug)]
pub enum Error {
    Df(format::Error),
    Io(io::Error),
}

pub struct SeekOverflow(());

impl fmt::Debug for SeekOverflow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SeekOverflow").finish()
    }
}

impl error::Error for SeekOverflow {
    fn description(&self) -> &str {
        "overflow while calculating seek offset"
    }
}

impl fmt::Display for SeekOverflow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        error::Error::description(self).fmt(f)
    }
}

impl From<format::Error> for Error {
    fn from(err: format::Error) -> Error {
        Error::Df(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<raw::Error<io::Error>> for Error {
    fn from(err: raw::Error<io::Error>) -> Error {
        match err {
            raw::Error::Df(e) => Error::Df(e),
            raw::Error::Cb(e) => Error::Io(e),
        }
    }
}

impl From<raw::WrapCallbackError<io::Error>> for Error {
    fn from(err: raw::WrapCallbackError<io::Error>) -> Error {
        let raw::WrapCallbackError(e) = err;
        Error::Io(e)
    }
}

struct CallbackDataNew {
    file: BufReader<File>,
    datafile_start: u64,
    cur_datafile_offset: u64,
    seek_base: Option<u64>,
}

struct CallbackData {
    file: File,
    seek_base: u64,
}

pub struct Reader {
    callback_data: CallbackData,
    raw: raw::Reader,
}

impl Reader {
    fn new_impl(file: File, check_initial_offset: bool) -> Result<Reader,Error> {
        let mut file = file;
        let datafile_start = if check_initial_offset {
            try!(file.seek(SeekFrom::Current(0)).wrap())
        } else {
            0
        };
        let mut callback_data_new = CallbackDataNew {
            file: BufReader::new(file),
            datafile_start: datafile_start,
            cur_datafile_offset: 0,
            seek_base: None,
        };
        let raw = try!(raw::Reader::new(&mut callback_data_new));
        let callback_data = CallbackData {
            file: callback_data_new.file.into_inner(),
            seek_base: callback_data_new.seek_base.unwrap(),
        };
        Ok(Reader { 
            callback_data: callback_data,
            raw: raw,
        })
    }
    pub fn new(file: File) -> Result<Reader,Error> {
        Reader::new_impl(file, true)
    }
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Reader,Error> {
        fn inner(path: &Path) -> Result<Reader,Error> {
            Reader::new_impl(try!(File::open(path)), false)
        }
        inner(path.as_ref())
    }
    pub fn debug_dump(&mut self) -> Result<(),Error> {
        Ok(try!(self.raw.debug_dump(&mut self.callback_data)))
    }
    pub fn version(&self) -> raw::Version {
        self.raw.version()
    }
    pub fn read_data(&mut self, index: usize) -> Result<Vec<u8>,Error> {
        Ok(try!(self.raw.read_data(&mut self.callback_data, index).map(|w| w.inner)))
    }
    pub fn item(&self, index: usize) -> ItemView {
        self.raw.item(index)
    }
    pub fn num_items(&self) -> usize {
        self.raw.num_items()
    }
    pub fn num_data(&self) -> usize {
        self.raw.num_data()
    }
    pub fn item_type_indices(&self, type_id: u16) -> ops::Range<usize> {
        self.raw.item_type_indices(type_id)
    }
    pub fn item_type(&self, index: usize) -> u16 {
        self.raw.item_type(index)
    }
    pub fn num_item_types(&self) -> usize {
        self.raw.num_item_types()
    }

    pub fn find_item(&self, type_id: u16, item_id: u16) -> Option<ItemView> {
        self.raw.find_item(type_id, item_id)
    }

    pub fn items(&self) -> raw::Items {
        self.raw.items()
    }
    pub fn item_types(&self) -> raw::ItemTypes {
        self.raw.item_types()
    }
    pub fn item_type_items(&self, type_id: u16) -> raw::ItemTypeItems {
        self.raw.item_type_items(type_id)
    }
    pub fn data_iter(&mut self) -> DataIter {
        fn map_fn(i: usize, self_: &mut &mut Reader) -> Result<Vec<u8>,Error> {
            self_.read_data(i)
        }
        let num_data = self.num_data();
        MapIterator::new(self, 0..num_data, map_fn)
    }
}

pub type DataIter<'a> = MapIterator<Result<Vec<u8>,Error>,&'a mut Reader,ops::Range<usize>>;

pub struct WrapVecU8 {
    inner: Vec<u8>,
}

impl DataCallback for WrapVecU8 {
    fn slice_mut(&mut self) -> &mut [u8] {
        &mut self.inner
    }
}

fn read<R: Read>(reader: &mut R, buffer: &mut [u8]) -> io::Result<usize> {
    let mut read = 0;
    while read != buffer.len() {
        match reader.read(&mut buffer[read..]) {
            Ok(0) => break,
            Ok(r) => read += r,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {},
            Err(e) => return Err(e),
        }
    }
    Ok(read)
}

fn read_offset(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    // Make sure the additions in this function don't overflow
    let _end_offset = try!(so(offset.checked_add(buffer.len().u64())));

    let mut read = 0;
    while read != buffer.len() {
        match file.read_offset(&mut buffer[read..], offset + read.u64()) {
            Ok(0) => break,
            Ok(r) => read += r,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {},
            Err(e) => return Err(e),
        }
    }
    Ok(read)
}

// "SeekOverflow"
fn so(o: Option<u64>) -> io::Result<u64> {
    o.ok_or(io::Error::new(io::ErrorKind::InvalidData, SeekOverflow(())))
}

impl CallbackNew for CallbackDataNew {
    type Error = io::Error;
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match read(&mut self.file, buffer) {
            Ok(r) => {
                self.cur_datafile_offset =
                    try!(so(self.cur_datafile_offset.checked_add(r.u64())));
                Ok(r)
            },
            Err(e) => Err(e),
        }
    }
    fn set_seek_base(&mut self) -> io::Result<()> {
        self.seek_base = Some(self.cur_datafile_offset);
        Ok(())
    }
    fn ensure_filesize(&mut self, filesize: u32) -> io::Result<Result<(),()>> {
        let actual = try!(self.file.get_ref().metadata()).len();
        Ok(if actual.checked_sub(self.datafile_start).unwrap() >= filesize.u64() {
            Ok(())
        } else {
            Err(())
        })
    }
}
impl CallbackReadData for CallbackData {
    type Error = io::Error;
    fn seek_read(&mut self, start: u32, buffer: &mut [u8]) -> io::Result<usize> {
        let offset = try!(so(self.seek_base.checked_add(start.u64())));
        read_offset(&self.file, buffer, offset)
    }
    type Data = WrapVecU8;
    fn alloc_data(&mut self, length: usize) -> io::Result<WrapVecU8> {
        let mut vec = Vec::with_capacity(length);
        unsafe { vec.set_len(length); }
        Ok(WrapVecU8 { inner: vec })
    }
}
