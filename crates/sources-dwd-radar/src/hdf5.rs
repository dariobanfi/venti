//! A reader for the HDF5 files DWD's radar composites come in, and for
//! nothing else.
//!
//! HDF5 is a large format. This reads the small part of it that the
//! recorded frames use and refuses every other part by name, so that a
//! file DWD writes differently one day fails loudly instead of being read
//! wrongly:
//!
//! - a superblock of version 0 with addresses and lengths of eight bytes;
//! - object headers of version 2, with their continuation blocks;
//! - groups whose links are stored in the header (a group of ODIM has a
//!   handful);
//! - attributes stored in the header, and attributes stored "densely", in a
//!   fractal heap whose root is one direct block, found through a B-tree of
//!   version 2 that is one leaf (the thirteen attributes of `/where`);
//! - scalar attributes that are a string, a little-endian IEEE double or a
//!   little-endian integer;
//! - one dataset: unsigned sixteen-bit integers, little-endian, in one
//!   chunk that is packed with deflate.
//!
//! Checksums in the file are not checked: a damaged file is caught by the
//! structure not fitting, by the bounds on every read, and by the adapter's
//! own checks of what it reads. The layouts are those of the HDF5 File
//! Format Specification, version 3.0, and were checked against the
//! recorded frames byte by byte when this was written (2026-11-21).
//!
//! Every read is bounded: an address outside the file, a length beyond its
//! end, more messages, attributes or blocks than a file of this kind has,
//! and data that unpacks to more than its dataset holds are all refused.

use std::io::Read;

/// An address that means "there is none".
const UNDEFINED: u64 = u64::MAX;

/// The most messages one object header may have. `/where` has 4, a group
/// with attributes in its header at most 12.
const MAX_MESSAGES: usize = 64;

/// The most continuation blocks one object header may have.
const MAX_BLOCKS: usize = 8;

/// The most attributes one object may have.
const MAX_ATTRIBUTES: usize = 64;

/// The longest attribute value that is read, in bytes. The longest in a
/// recorded frame is the projection's definition, 142 bytes.
const MAX_VALUE_BYTES: usize = 4096;

const SIGNATURE: &[u8; 8] = b"\x89HDF\r\n\x1a\n";

const MESSAGE_DATASPACE: u8 = 0x01;
const MESSAGE_LINK_INFO: u8 = 0x02;
const MESSAGE_DATATYPE: u8 = 0x03;
const MESSAGE_LINK: u8 = 0x06;
const MESSAGE_LAYOUT: u8 = 0x08;
const MESSAGE_FILTERS: u8 = 0x0b;
const MESSAGE_ATTRIBUTE: u8 = 0x0c;
const MESSAGE_CONTINUATION: u8 = 0x10;
const MESSAGE_ATTRIBUTE_INFO: u8 = 0x15;

/// The value of a scalar attribute.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// A string, without its terminator and trailing spaces.
    Text(String),
    /// A floating-point number.
    Float(f64),
    /// A whole number.
    Int(i64),
}

impl Value {
    /// The value as text, if it is text.
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The value as a number, if it is one. A whole number of a size a
    /// double holds exactly.
    pub(crate) fn number(&self) -> Option<f64> {
        match self {
            Value::Float(value) => Some(*value),
            // Sizes and counts: far below 2^53.
            #[allow(clippy::cast_precision_loss)]
            Value::Int(value) if value.unsigned_abs() < (1 << 53) => Some(*value as f64),
            _ => None,
        }
    }
}

/// One message of an object header: its kind, its flags and its bytes.
#[derive(Debug, Clone, Copy)]
struct Message<'a> {
    kind: u8,
    flags: u8,
    data: &'a [u8],
}

/// An object of the file: a group or a dataset, as the messages of its
/// header.
#[derive(Debug, Clone)]
pub(crate) struct Object<'a> {
    messages: Vec<Message<'a>>,
}

/// An HDF5 file held in memory.
#[derive(Debug, Clone, Copy)]
pub(crate) struct File<'a> {
    bytes: &'a [u8],
    root: u64,
}

type Result<T> = std::result::Result<T, String>;

/// Reads little-endian numbers out of a slice, each read checked.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("the file ends inside a structure")?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn skip(&mut self, length: usize) -> Result<()> {
        self.take(length).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        let mut eight = [0u8; 8];
        eight.copy_from_slice(b);
        Ok(u64::from_le_bytes(eight))
    }

    /// A number of `width` bytes, at most eight.
    fn uint(&mut self, width: usize) -> Result<u64> {
        if width > 8 {
            return Err("a number wider than eight bytes".to_owned());
        }
        let mut eight = [0u8; 8];
        eight[..width].copy_from_slice(self.take(width)?);
        Ok(u64::from_le_bytes(eight))
    }

    fn rest(&self) -> &'a [u8] {
        &self.bytes[self.at..]
    }
}

impl<'a> File<'a> {
    /// Opens a file: checks that it is HDF5 of the form this reader
    /// knows, and finds its root group.
    pub(crate) fn open(bytes: &'a [u8]) -> Result<Self> {
        let mut cursor = Cursor::new(bytes);
        if cursor.take(8).ok() != Some(SIGNATURE.as_slice()) {
            return Err("not an HDF5 file: it does not begin with HDF5's signature".to_owned());
        }
        let version = cursor.u8()?;
        if version != 0 {
            return Err(format!(
                "an HDF5 superblock of version {version}; this reader knows version 0"
            ));
        }
        // The versions of the free-space storage, of the root group's
        // entry and of the shared header messages, and two reserved bytes
        // around them.
        cursor.skip(4)?;
        let (offsets, lengths) = (cursor.u8()?, cursor.u8()?);
        if (offsets, lengths) != (8, 8) {
            return Err(format!(
                "addresses of {offsets} and lengths of {lengths} bytes; this reader knows 8 and 8"
            ));
        }
        // A reserved byte, the two sizes of a group's nodes and the
        // consistency flags.
        cursor.skip(1 + 2 + 2 + 4)?;
        let base = cursor.u64()?;
        if base != 0 {
            return Err("the file's addresses do not begin at its first byte".to_owned());
        }
        // Where the free space is described.
        cursor.skip(8)?;
        let end = cursor.u64()?;
        if end > bytes.len() as u64 {
            return Err(format!(
                "the file says it has {end} bytes and has {}: it is cut short",
                bytes.len()
            ));
        }
        // The driver's block, and in the root group's entry the offset of
        // its name.
        cursor.skip(8 + 8)?;
        let root = cursor.u64()?;
        Ok(Self { bytes, root })
    }

    /// The bytes from an address on.
    fn at(&self, address: u64) -> Result<Cursor<'a>> {
        let start = usize::try_from(address)
            .ok()
            .filter(|start| *start < self.bytes.len())
            .ok_or("an address outside the file")?;
        Ok(Cursor::new(&self.bytes[start..]))
    }

    /// The object at the end of a path of group names from the root.
    pub(crate) fn object(&self, path: &[&str]) -> Result<Object<'a>> {
        let mut object = self.header(self.root)?;
        for name in path {
            let address = self
                .links(&object)?
                .into_iter()
                .find(|(link, _)| link == name)
                .map(|(_, address)| address)
                .ok_or_else(|| format!("the file has no /{}", path.join("/")))?;
            object = self.header(address)?;
        }
        Ok(object)
    }

    /// Reads the object header at an address: every message of it,
    /// through its continuation blocks.
    fn header(&self, address: u64) -> Result<Object<'a>> {
        let mut cursor = self.at(address)?;
        if cursor.take(4)? != b"OHDR" {
            return Err(
                "an object header of the old form; this reader knows version 2 (OHDR)".to_owned(),
            );
        }
        let version = cursor.u8()?;
        if version != 2 {
            return Err(format!("an object header of version {version}"));
        }
        let flags = cursor.u8()?;
        if flags & 0x20 != 0 {
            // Four times: of access, modification, change and birth.
            cursor.skip(16)?;
        }
        if flags & 0x10 != 0 {
            // From how many attributes on they are stored densely.
            cursor.skip(4)?;
        }
        let size = cursor.uint(1 << (flags & 0x03))?;
        let size = usize::try_from(size).map_err(|_| "an object header too large")?;
        // With this flag each message carries the order it was made in.
        let order_bytes = if flags & 0x04 != 0 { 2 } else { 0 };

        let mut messages = Vec::new();
        let mut blocks = vec![cursor.take(size)?];
        let mut read = 0;
        while read < blocks.len() {
            let mut block = Cursor::new(blocks[read]);
            read += 1;
            // What is left when less than a message's head remains is a gap.
            while block.rest().len() >= 4 + order_bytes {
                let kind = block.u8()?;
                let length = usize::from(block.u16()?);
                let message_flags = block.u8()?;
                block.skip(order_bytes)?;
                let data = block.take(length)?;
                if kind == MESSAGE_CONTINUATION {
                    if blocks.len() >= MAX_BLOCKS {
                        return Err("an object header in too many blocks".to_owned());
                    }
                    let mut continuation = Cursor::new(data);
                    let (address, length) = (continuation.u64()?, continuation.u64()?);
                    let length = usize::try_from(length).map_err(|_| "a block too large")?;
                    let mut next = self.at(address)?;
                    if next.take(4)? != b"OCHK" {
                        return Err("a continuation block without its signature".to_owned());
                    }
                    // The length takes in the signature and a checksum.
                    let inner = length.checked_sub(8).ok_or("a block too short")?;
                    blocks.push(next.take(inner)?);
                    continue;
                }
                if messages.len() >= MAX_MESSAGES {
                    return Err("an object header with too many messages".to_owned());
                }
                messages.push(Message {
                    kind,
                    flags: message_flags,
                    data,
                });
            }
        }
        Ok(Object { messages })
    }

    /// The links of a group: the name and the address of each object in it.
    fn links(&self, group: &Object<'a>) -> Result<Vec<(String, u64)>> {
        let mut links = Vec::new();
        for message in &group.messages {
            match message.kind {
                MESSAGE_LINK_INFO => {
                    let mut cursor = Cursor::new(message.data);
                    let (_version, flags) = (cursor.u8()?, cursor.u8()?);
                    if flags & 0x01 != 0 {
                        cursor.skip(8)?;
                    }
                    if cursor.u64()? != UNDEFINED {
                        return Err(
                            "a group whose links are stored outside its header; this reader \
                             follows the links in the header"
                                .to_owned(),
                        );
                    }
                }
                MESSAGE_LINK => {
                    own(message)?;
                    let mut cursor = Cursor::new(message.data);
                    let (version, flags) = (cursor.u8()?, cursor.u8()?);
                    if version != 1 {
                        return Err(format!("a link of version {version}"));
                    }
                    let hard = if flags & 0x08 != 0 {
                        cursor.u8()? == 0
                    } else {
                        true
                    };
                    if flags & 0x04 != 0 {
                        cursor.skip(8)?;
                    }
                    if flags & 0x10 != 0 {
                        cursor.skip(1)?;
                    }
                    let length = cursor.uint(1 << (flags & 0x03))?;
                    let length = usize::try_from(length).map_err(|_| "a name too long")?;
                    let name = cursor.take(length)?;
                    if !hard {
                        // A link to somewhere else: nothing this reader follows.
                        continue;
                    }
                    let address = cursor.u64()?;
                    links.push((String::from_utf8_lossy(name).into_owned(), address));
                }
                _ => {}
            }
        }
        Ok(links)
    }

    /// The scalar attributes of an object, by name. An attribute that is
    /// not a scalar string, double or integer is left out.
    pub(crate) fn attributes(&self, object: &Object<'a>) -> Result<Vec<(String, Value)>> {
        let mut attributes = Vec::new();
        let mut take = |bytes: &'a [u8]| -> Result<()> {
            if attributes.len() >= MAX_ATTRIBUTES {
                return Err("an object with too many attributes".to_owned());
            }
            if let Some(attribute) = attribute(bytes)? {
                attributes.push(attribute);
            }
            Ok(())
        };
        for message in &object.messages {
            match message.kind {
                MESSAGE_ATTRIBUTE => {
                    own(message)?;
                    take(message.data)?;
                }
                MESSAGE_ATTRIBUTE_INFO => {
                    let mut cursor = Cursor::new(message.data);
                    let (_version, flags) = (cursor.u8()?, cursor.u8()?);
                    if flags & 0x01 != 0 {
                        cursor.skip(2)?;
                    }
                    let (heap, names) = (cursor.u64()?, cursor.u64()?);
                    if heap != UNDEFINED {
                        for bytes in self.dense_attributes(heap, names)? {
                            take(bytes)?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(attributes)
    }

    /// The attributes an object keeps outside its header: each as the
    /// bytes of an attribute message, out of a fractal heap, found through
    /// the B-tree that indexes them by name.
    fn dense_attributes(&self, heap: u64, names: u64) -> Result<Vec<&'a [u8]>> {
        // The heap's header.
        let mut cursor = self.at(heap)?;
        if cursor.take(4)? != b"FRHP" || cursor.u8()? != 0 {
            return Err("a fractal heap this reader does not know".to_owned());
        }
        let id_length = usize::from(cursor.u16()?);
        let filters_length = cursor.u16()?;
        let heap_flags = cursor.u8()?;
        let max_managed = u64::from(cursor.u32()?);
        // Huge objects, free space, the sizes and counts of what the heap
        // holds: twelve numbers of eight bytes.
        cursor.skip(12 * 8)?;
        let _table_width = cursor.u16()?;
        let _starting_block = cursor.u64()?;
        let max_direct = cursor.u64()?;
        let max_heap_bits = usize::from(cursor.u16()?);
        let _starting_rows = cursor.u16()?;
        let root = cursor.u64()?;
        let rows_of_root = cursor.u16()?;
        if filters_length != 0 {
            return Err("a fractal heap that is packed".to_owned());
        }
        if rows_of_root != 0 {
            return Err(
                "a fractal heap of more than one block; this reader knows a heap that is one \
                 direct block"
                    .to_owned(),
            );
        }
        // How an object's place in the heap is written in its identifier:
        // the offset in as many bytes as the heap's size needs, the length
        // in as many as the largest object needs.
        let offset_bytes = max_heap_bits.div_ceil(8);
        let largest = max_direct.min(max_managed).max(1);
        let length_bytes = (largest.ilog2() / 8 + 1) as usize;
        if 1 + offset_bytes + length_bytes > id_length || offset_bytes > 8 {
            return Err("a fractal heap whose identifiers do not fit".to_owned());
        }

        // The direct block: its signature, a version, the heap's address,
        // its own offset in the heap, and a checksum if the heap has them.
        let mut block = self.at(root)?;
        if block.take(4)? != b"FHDB" || block.u8()? != 0 {
            return Err("a heap block this reader does not know".to_owned());
        }
        if block.u64()? != heap {
            return Err("a heap block of another heap".to_owned());
        }
        if block.uint(offset_bytes)? != 0 {
            return Err("a heap's root block that is not its first".to_owned());
        }
        let _ = heap_flags;

        // The B-tree of the names: a header and, with a depth of zero, one
        // leaf of records.
        let mut tree = self.at(names)?;
        if tree.take(4)? != b"BTHD" || tree.u8()? != 0 {
            return Err("a B-tree this reader does not know".to_owned());
        }
        let kind = tree.u8()?;
        let _node_size = tree.u32()?;
        let record_size = usize::from(tree.u16()?);
        let depth = tree.u16()?;
        // At what fill a node is split and merged.
        tree.skip(2)?;
        let leaf = tree.u64()?;
        let records = usize::from(tree.u16()?);
        if kind != 8 {
            return Err(format!(
                "a B-tree of kind {kind} where attributes' names are indexed"
            ));
        }
        if depth != 0 {
            return Err("a B-tree of more than one node".to_owned());
        }
        if records > MAX_ATTRIBUTES {
            return Err("an object with too many attributes".to_owned());
        }
        // A record: the heap's identifier of the attribute, the message's
        // flags, the order it was made in, a hash of its name.
        if record_size != id_length + 1 + 4 + 4 {
            return Err("a B-tree whose records are of another size".to_owned());
        }
        let mut node = self.at(leaf)?;
        if node.take(4)? != b"BTLF" || node.u8()? != 0 || node.u8()? != kind {
            return Err("a B-tree leaf this reader does not know".to_owned());
        }

        let root_start = usize::try_from(root).map_err(|_| "an address outside the file")?;
        let mut found = Vec::with_capacity(records);
        for _ in 0..records {
            let mut record = Cursor::new(node.take(record_size)?);
            let head = record.u8()?;
            // The identifier's first byte: its version and the kind of
            // object. Zero is an object in the heap's blocks.
            if head & 0xf0 != 0 {
                return Err("an attribute kept outside the heap's blocks".to_owned());
            }
            let offset = record.uint(offset_bytes)?;
            let length = record.uint(length_bytes)?;
            let start = usize::try_from(offset)
                .ok()
                .and_then(|offset| root_start.checked_add(offset))
                .ok_or("an attribute outside the file")?;
            let end = usize::try_from(length)
                .ok()
                .and_then(|length| start.checked_add(length))
                .filter(|end| *end <= self.bytes.len())
                .ok_or("an attribute outside the file")?;
            found.push(&self.bytes[start..end]);
        }
        Ok(found)
    }

    /// The values of a dataset of unsigned sixteen-bit integers of `rows`
    /// by `columns`, row by row. Refused if the dataset is of another
    /// size or another type, or stored in a way this reader does not know.
    pub(crate) fn grid_u16(
        &self,
        dataset: &Object<'a>,
        rows: usize,
        columns: usize,
    ) -> Result<Vec<u16>> {
        let message = |kind: u8, what: &str| -> Result<&'a [u8]> {
            let message = dataset
                .messages
                .iter()
                .find(|message| message.kind == kind)
                .ok_or_else(|| format!("the dataset has no {what}"))?;
            own(message)?;
            Ok(message.data)
        };

        let dimensions = dimensions(message(MESSAGE_DATASPACE, "dataspace")?)?;
        if dimensions != [rows as u64, columns as u64] {
            return Err(format!(
                "the dataset is {dimensions:?}, not {rows} rows of {columns}"
            ));
        }
        match datatype(message(MESSAGE_DATATYPE, "datatype")?)? {
            Type::Int {
                size: 2,
                signed: false,
            } => {}
            other => {
                return Err(format!(
                    "the dataset holds {other:?}, not unsigned 16-bit integers"
                ));
            }
        }
        let filters = dataset
            .messages
            .iter()
            .find(|message| message.kind == MESSAGE_FILTERS)
            .map(|message| filters(message.data))
            .transpose()?
            .unwrap_or_default();
        if filters != [FILTER_DEFLATE] {
            return Err(format!(
                "the dataset is packed with the filters {filters:?}; this reader knows \
                 deflate alone"
            ));
        }

        // The layout: one chunk as large as the dataset, found through a
        // B-tree of version 1 that is one leaf with one entry.
        let mut layout = Cursor::new(message(MESSAGE_LAYOUT, "layout")?);
        let (version, class) = (layout.u8()?, layout.u8()?);
        if (version, class) != (3, 2) {
            return Err(format!(
                "a dataset laid out as version {version}, class {class}; this reader knows \
                 version 3 in chunks"
            ));
        }
        let rank = usize::from(layout.u8()?);
        let tree = layout.u64()?;
        // A chunk's size has one number more than the dataset has
        // dimensions: the size of a value.
        if rank != 3 {
            return Err(format!("chunks of {rank} dimensions"));
        }
        let chunk = [layout.u32()?, layout.u32()?, layout.u32()?];
        if chunk.map(u64::from) != [rows as u64, columns as u64, 2] {
            return Err(format!(
                "the dataset is in chunks of {chunk:?}; this reader knows one chunk of the \
                 whole"
            ));
        }
        let mut node = self.at(tree)?;
        if node.take(4)? != b"TREE" {
            return Err("the chunks' index is no B-tree".to_owned());
        }
        let (kind, level, entries) = (node.u8()?, node.u8()?, node.u16()?);
        if (kind, level, entries) != (1, 0, 1) {
            return Err("the chunks' index is not one leaf with one chunk".to_owned());
        }
        // The node's two neighbours.
        node.skip(16)?;
        let packed_length = node.u32()? as usize;
        let skipped_filters = node.u32()?;
        // Where the chunk begins in the dataset: three zeros.
        node.skip(3 * 8)?;
        let address = node.u64()?;
        if skipped_filters != 0 {
            return Err("a chunk that left a filter out".to_owned());
        }
        let packed = self.at(address)?.take(packed_length)?;

        let bytes = inflate_exactly(packed, rows * columns * 2)?;
        Ok(bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect())
    }
}

/// Refuses a message that is kept elsewhere and shared: its bytes are then
/// a pointer, not the message.
fn own(message: &Message<'_>) -> Result<()> {
    if message.flags & 0x02 != 0 {
        return Err(
            "a header message that is shared; this reader knows messages kept in \
                    their header"
                .to_owned(),
        );
    }
    Ok(())
}

/// Unpacks a zlib stream that must hold exactly `length` bytes. Reading
/// stops one byte past that, so a stream that holds more is refused
/// without being unpacked.
pub(crate) fn inflate_exactly(packed: &[u8], length: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(length);
    flate2::read::ZlibDecoder::new(packed)
        .take(length as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("the data cannot be unpacked: {error}"))?;
    if bytes.len() != length {
        return Err(format!(
            "the data unpacks to {} bytes{}, and its dataset holds {length}",
            bytes.len(),
            if bytes.len() > length {
                " and more"
            } else {
                ""
            },
        ));
    }
    Ok(bytes)
}

/// The identifier of the deflate filter.
const FILTER_DEFLATE: u16 = 1;

/// The identifiers of the filters a dataset is packed with, in order.
fn filters(bytes: &[u8]) -> Result<Vec<u16>> {
    let mut cursor = Cursor::new(bytes);
    let (version, count) = (cursor.u8()?, cursor.u8()?);
    if count > 8 {
        return Err("a dataset with too many filters".to_owned());
    }
    let mut found = Vec::new();
    match version {
        1 => {
            cursor.skip(6)?;
            for _ in 0..count {
                let id = cursor.u16()?;
                let name_length = usize::from(cursor.u16()?);
                let _flags = cursor.u16()?;
                let values = usize::from(cursor.u16()?);
                // The name is padded to a multiple of eight bytes, and the
                // values to an even count.
                cursor.skip(name_length.div_ceil(8) * 8)?;
                cursor.skip((values + values % 2) * 4)?;
                found.push(id);
            }
        }
        2 => {
            for _ in 0..count {
                let id = cursor.u16()?;
                let name_length = if id >= 256 {
                    usize::from(cursor.u16()?)
                } else {
                    0
                };
                let _flags = cursor.u16()?;
                let values = usize::from(cursor.u16()?);
                cursor.skip(name_length)?;
                cursor.skip(values * 4)?;
                found.push(id);
            }
        }
        other => return Err(format!("a filter pipeline of version {other}")),
    }
    Ok(found)
}

/// The type of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Type {
    /// A little-endian integer of `size` bytes.
    Int { size: usize, signed: bool },
    /// A little-endian IEEE float of `size` bytes.
    Float { size: usize },
    /// A string of `size` bytes.
    Text { size: usize },
    /// Anything else.
    Other,
}

fn datatype(bytes: &[u8]) -> Result<Type> {
    let mut cursor = Cursor::new(bytes);
    let class = cursor.u8()? & 0x0f;
    let bits = cursor.u8()?;
    cursor.skip(2)?;
    let size = cursor.u32()? as usize;
    // Bit 0 of an integer's and a float's flags is its byte order.
    let little_endian = bits & 0x01 == 0;
    Ok(match class {
        0 if little_endian && matches!(size, 1 | 2 | 4 | 8) => Type::Int {
            size,
            signed: bits & 0x08 != 0,
        },
        1 if little_endian && matches!(size, 4 | 8) => Type::Float { size },
        3 => Type::Text { size },
        _ => Type::Other,
    })
}

/// The sizes of a dataspace's dimensions. None for a scalar.
fn dimensions(bytes: &[u8]) -> Result<Vec<u64>> {
    let mut cursor = Cursor::new(bytes);
    let version = cursor.u8()?;
    let rank = usize::from(cursor.u8()?);
    match version {
        // Flags, and five reserved bytes.
        1 => cursor.skip(6)?,
        // Flags, and the kind of space.
        2 => cursor.skip(2)?,
        other => return Err(format!("a dataspace of version {other}")),
    }
    if rank > 4 {
        return Err(format!("a dataspace of {rank} dimensions"));
    }
    (0..rank).map(|_| cursor.u64()).collect()
}

/// Reads an attribute message: its name and, if it is a scalar this
/// reader knows, its value.
fn attribute(bytes: &[u8]) -> Result<Option<(String, Value)>> {
    let mut cursor = Cursor::new(bytes);
    let version = cursor.u8()?;
    // Reserved in version 1, flags from version 2 on: bits 0 and 1 say
    // that the type or the space is shared, which is a pointer elsewhere.
    let flags = cursor.u8()?;
    let name_length = usize::from(cursor.u16()?);
    let type_length = usize::from(cursor.u16()?);
    let space_length = usize::from(cursor.u16()?);
    let padded = |length: usize| match version {
        1 => length.div_ceil(8) * 8,
        _ => length,
    };
    match version {
        1 => {}
        2 if flags & 0x03 == 0 => {}
        3 if flags & 0x03 == 0 => cursor.skip(1)?,
        _ => return Err(format!("an attribute of version {version}, flags {flags}")),
    }
    let name = cursor.take(padded(name_length))?;
    let name = &name[..name_length.min(name.len())];
    let name = name.split(|byte| *byte == 0).next().unwrap_or_default();
    let name = String::from_utf8_lossy(name).into_owned();
    let kind = datatype(&cursor.take(padded(type_length))?[..type_length])?;
    let scalar = dimensions(&cursor.take(padded(space_length))?[..space_length])?.is_empty();
    let value = cursor.rest();
    if !scalar {
        return Ok(None);
    }
    let sized = |size: usize| -> Result<&[u8]> {
        if size > MAX_VALUE_BYTES {
            return Err(format!("the attribute {name} is {size} bytes long"));
        }
        value
            .get(..size)
            .ok_or_else(|| format!("the attribute {name} is cut short"))
    };
    let value = match kind {
        Type::Text { size } => {
            let text = sized(size)?;
            let text = text.split(|byte| *byte == 0).next().unwrap_or_default();
            Value::Text(String::from_utf8_lossy(text).trim_end().to_owned())
        }
        Type::Float { size: 8 } => {
            let mut eight = [0u8; 8];
            eight.copy_from_slice(sized(8)?);
            Value::Float(f64::from_le_bytes(eight))
        }
        Type::Float { .. } => {
            let b = sized(4)?;
            Value::Float(f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
        }
        Type::Int { size, signed } => {
            let mut eight = [0u8; 8];
            let b = sized(size)?;
            eight[..size].copy_from_slice(b);
            // A negative number fills the bytes above its own with ones.
            if signed && b[size - 1] & 0x80 != 0 {
                eight[size..].fill(0xff);
            }
            Value::Int(i64::from_le_bytes(eight))
        }
        Type::Other => return Ok(None),
    };
    Ok(Some((name, value)))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn data_that_unpacks_to_more_than_its_dataset_holds_is_refused_unread() {
        let pack = |bytes: &[u8]| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        };
        // Sixteen megabytes of zeros in sixteen kilobytes.
        let bomb = pack(&vec![0u8; 16 * 1024 * 1024]);
        assert!(bomb.len() < 20_000);

        let said = inflate_exactly(&bomb, 1000).unwrap_err();
        assert!(said.contains("1001 bytes and more"), "{said}");

        assert_eq!(inflate_exactly(&pack(&[7; 1000]), 1000).unwrap(), [7; 1000]);
        let short = inflate_exactly(&pack(&[7; 999]), 1000).unwrap_err();
        assert!(short.contains("999 bytes,"), "{short}");
        assert!(inflate_exactly(b"not zlib", 8).is_err());
    }

    #[test]
    fn what_is_not_this_kind_of_hdf5_is_refused_by_name() {
        assert!(
            File::open(b"<html>404</html>")
                .unwrap_err()
                .contains("not an HDF5 file")
        );
        // A superblock of version 2, as newer libraries write by default.
        let mut newer = SIGNATURE.to_vec();
        newer.extend([2, 8, 8, 0]);
        newer.extend([0u8; 64]);
        assert!(
            File::open(&newer)
                .unwrap_err()
                .contains("superblock of version 2")
        );
        // Version 0, and a size of the file that it does not have.
        let mut cut = SIGNATURE.to_vec();
        cut.extend([0, 0, 0, 0, 0, 8, 8, 0, 4, 0, 16, 0, 0, 0, 0, 0]);
        cut.extend(0u64.to_le_bytes());
        cut.extend(u64::MAX.to_le_bytes());
        cut.extend(5000u64.to_le_bytes());
        cut.extend([0u8; 48]);
        assert!(File::open(&cut).unwrap_err().contains("cut short"));
    }

    #[test]
    fn numbers_are_read_within_their_bounds() {
        let mut cursor = Cursor::new(&[1, 2, 3, 4, 5]);
        assert_eq!(cursor.u16().unwrap(), 0x0201);
        assert_eq!(cursor.uint(3).unwrap(), 0x05_0403);
        assert!(cursor.u8().is_err());
        assert!(Cursor::new(&[0; 16]).uint(9).is_err());
    }
}
