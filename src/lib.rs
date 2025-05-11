//! rawpsd is a library that handles loading PSD data into a list of minimally-processed in-memory structs. It does not have any opinions about what features PSD files should or do use, or how to interpret those features. Compressed data is decompressed, and some redundant pieces of data like ascii and unicode names stored together are only returned once instead of twice, but aside from things like that, rawpsd is minimally opinionated and tries to just tell you what the PSD file itself says. For example, strings are left as strings instead of being transformed into enums.
//!
//! Comparison with other crates:
//! - `psd`: The `psd` crate's API makes it impossible to figure out the exact layer group hierarchy, so you can only use it on very simple PSDs.
//! - `zune-psd`: Doesn't actually support the psd format, just gets the embedded thumbnail.
//!
//! rawpsd draws a compatibility support line at Photoshop CS6, the last non-subscription version of Photoshop. Features only supported by newer versions are unlikely to be supported.
//!
//! rawpsd currently only supports 8-bit RGB, CMYK, and Grayscale PSDs. This is the vast majority of PSD files that can be found in the wild. It does not yet support the large document PSB format variant.
//!
//! rawpsd's docs do not document the entire PSD format, not even its capabilities. You will need to occasionally reference <https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/> and manually poke at PSD files in a hex editor to take full advantage of rawpsd.
//!
//! You want [parse_layer_records] and [parse_psd_metadata].
//!
//! Example:
//!
//!```rs
//!let data = std::fs::read("data/test.psd").expect("Failed to open test.psd");
//!
//!if let Ok(layers) = parse_layer_records(&data)
//!{
//!    for mut layer in layers
//!    {
//!        // Don't spew tons of image data bytes to stdout; we just want to see the metadata.
//!        layer.image_data_rgba = vec!();
//!        layer.image_data_k = vec!();
//!        layer.image_data_mask = vec!();
//!        println!("{:?}", layer);
//!    }
//!}
//!```

#![allow(clippy::vec_init_then_push)] // wrong problem domain. more readable this way
#![allow(clippy::manual_range_contains)] // bad idiom
#![allow(clippy::field_reassign_with_default)] // bad idiom

#![cfg_attr(not(any(test, feature = "serde_support", feature = "debug_spew")), no_std)]
extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::format;

#[derive(Clone, Debug, Default)]
struct SliceCursor<'a>
{
    pub (crate) buf : &'a [u8],
    pub (crate) pos : usize,
}

impl<'a> SliceCursor<'a>
{
    pub (crate) fn new(buf : &'a [u8]) -> Self
    {
        Self { buf, pos: 0 }
    }

    pub (crate) fn position(&self) -> u64 { self.pos as u64 }
    pub (crate) fn set_position(&mut self, pos : u64) { self.pos = pos as usize }
    
    pub (crate) fn read_exact(&mut self, out : &mut [u8]) -> Result<(), String>
    {
        let remaining = self.buf.len().saturating_sub(self.pos);
        if out.len() > remaining
        {
            return Err("Unexpeted end of stream".to_string());
        }
        out.copy_from_slice(&self.buf[self.pos..self.pos + out.len()]);
        self.pos += out.len();
        Ok(())
    }

    pub (crate) fn read_to_end(&mut self, out : &mut Vec<u8>) -> Result<usize, String>
    {
        let remaining = self.buf.len().saturating_sub(self.pos);
        out.reserve(remaining);
        out.extend_from_slice(&self.buf[self.pos..]);
        self.pos = self.buf.len();
        Ok(remaining)
    }
    
    pub fn take(&mut self, n : u64) -> Self
    {
        Self { buf : &self.buf[self.pos..self.pos + n as usize], pos : 0 }
    }
    
    pub fn take_rest(&mut self) -> Self
    {
        Self { buf : &self.buf[self.pos..], pos : 0 }
    }
     
}

use alloc::collections::BTreeMap;

/// PSD Class Descriptor object data. Only used by certain PSD features.
///
/// Some PSD format features use a dynamic meta-object format instead of feature-specific data encoding; that information is what this type is responsible for holding.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub enum DescItem
{
    #[allow(non_camel_case_types)]
    long(i32),
    #[allow(non_camel_case_types)]
    doub(f64),
    /// Float that carries unit system metadata. The string specifies the unit system. Examples of unit systems are "#Ang" and "#Pxl".
    UntF(String, f64),
    #[allow(non_camel_case_types)]
    bool(bool),
    TEXT(String),
    /// When rawpsd ran into an error while parsing the data that goes here: what kind of error was it?
    Err(String),
    /// Entire sub-object.
    Objc(Box<Descriptor>),
    #[allow(non_camel_case_types)]
    /// Enums, which are stringly typed in PSDs.
    _enum(String, String),
    /// Variable-length list.
    VlLs(Vec<DescItem>),
    /// Dummy non-data data.
    #[default] Xxx
}

impl DescItem
{
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn long(&self) -> i32 { match self { DescItem::long(x) => *x, _ => panic!(), } }
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn doub(&self) -> f64 { match self { DescItem::doub(x) => *x, _ => panic!(), } }
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn bool(&self) -> bool { match self { DescItem::bool(x) => *x, _ => panic!(), } }
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn _enum(&self) -> (String, String) { match self { DescItem::_enum(y, x) => (y.clone(), x.clone()), _ => panic!(), } }
    #[allow(non_snake_case)]
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn UntF(&self) -> (String, f64) { match self { DescItem::UntF(y, x) => (y.clone(), *x), _ => panic!(), } }
    #[allow(non_snake_case)]
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn Objc(&self) -> Box<Descriptor> { match self { DescItem::Objc(x) => x.clone(), _ => panic!(), } }
    #[allow(non_snake_case)]
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn TEXT(&self) -> String { match self { DescItem::TEXT(x) => x.clone(), _ => panic!(), } }
    #[allow(non_snake_case)]
    /// Get the given item if the enum is of that kind, otherwise panic.
    pub fn VlLs(&self) -> Vec<DescItem> { match self { DescItem::VlLs(x) => x.clone(), _ => panic!(), } }
}

type Descriptor = (String, Vec<(String, DescItem)>);

#[cfg(feature = "serde_support")]
use serde::{Serialize, Deserialize};
#[cfg(feature = "serde_support")]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
/// Metadata about where a mask attached to an object physically is and how to interpret it.
///
/// Stability promise: Every field in this struct will always be public. This struct is safe to initialize with `{ ..., ..Default::default() }`.
///
/// This struct is general purpose enough that you might want to use it in your code directly instead of making a newtype. If you do, and you need to serde it, enable the `serde_support` feature. The serde format of this struct is not guaranteed to be stable between minor versions or patch versions; if you use the `serde_support` feature and need to ensure compatibility between different builds of your code, pin `rawpsd` to a specific exact version. Otherwise, make a newtype.
#[non_exhaustive]
pub struct MaskInfo {
    /// Mask global X position.
    pub x : i32,
    /// Mask global Y position.
    pub y : i32,
    /// Mask image data width.
    pub w : u32,
    /// Mask image data height.
    pub h : u32,
    /// Default color of the mask outside of its explicit image area. Must be 0 or 255.
    pub default_color : u8,
    /// Is the mask flagged as moving along with the layer it's attached to? Does not affect the encoding of the x/y coordinates. I think.
    pub relative : bool,
    /// Is the mask disabled, i.e. marked as currently having no effect?
    pub disabled : bool,
    /// Is the mask marked as inverted, i.e. the default color and image data should be treated as being the opposite color (white <-> black)?
    pub invert : bool,
}

#[cfg(not(feature = "serde_support"))]
#[derive(Clone, Debug, Default)]
/// Metadata about where a mask attached to an object physically is and how to interpret it.
///
/// Stability promise: Every field in this struct will always be public. This struct is safe to initialize with `{ ..., ..Default::default() }`.
///
/// This struct is general purpose enough that you might want to use it in your code directly instead of making a newtype. If you do, and you need to serde it, enable the `serde_support` feature. The serde format of this struct is not guaranteed to be stable between minor versions or patch versions; if you use the `serde_support` feature and need to ensure compatibility between different builds of your code, pin `rawpsd` to a specific exact version. Otherwise, make a newtype.
#[non_exhaustive]
pub struct MaskInfo {
    /// Mask global X position.
    pub x : i32,
    /// Mask global Y position.
    pub y : i32,
    /// Mask image data width.
    pub w : u32,
    /// Mask image data height.
    pub h : u32,
    /// Default color of the mask outside of its explicit image area. Must be 0 or 255.
    pub default_color : u8,
    /// Is the mask flagged as moving along with the layer it's attached to? Does not affect the encoding of the x/y coordinates. I think.
    pub relative : bool,
    /// Is the mask disabled, i.e. marked as currently having no effect?
    pub disabled : bool,
    /// Is the mask marked as inverted, i.e. the default color and image data should be treated as being the opposite color (white <-> black)?
    pub invert : bool,
}

/// Dummy struct to keep the main docs from being bloated. See [LayerInfo::blend_mode].
///
/// Normal blend modes:
/// ```text
///     "pass" => "Normal", // "Pass through" mode for groups. Does not behave as a normal blend mode. Affects composition pipeline behavior.
///     "norm" => "Normal",
///     "diss" => "Dissolve",
///     "dark" => "Darken",
///     "mul " => "Multiply",
///     "idiv" => "Color Burn",
///     "lbrn" => "Linear Burn",
///     "dkCl" => "Darken",
///     "lite" => "Lighten",
///     "scrn" => "Screen",
///     "div " => "Color Dodge",
///     "lddg" => "Add",
///     "lgCl" => "Lighten",
///     "over" => "Overlay",
///     "sLit" => "Soft Light",
///     "hLit" => "Hard Light",
///     "vLit" => "Vivid Light",
///     "lLit" => "Linear Light",
///     "pLit" => "Pin Light",
///     "hMix" => "Hard Mix",
///     "diff" => "Difference",
///     "smud" => "Exclusion",
///     "fsub" => "Subtract",
///     "fdiv" => "Divide",
///     "hue " => "Hue",
///     "sat " => "Saturation",
///     "colr" => "Color",
///     "lum " => "Luminosity",
///     _ => "Normal",
/// ```
/// Blend modes as found in certain Class Descriptor objects in certain effect/filter-related features:
/// ```text
///     "Nrml" => "Normal",
///     "Dslv" => "Dissolve",
///     "Drkn" => "Darken",
///     "Mltp" => "Multiply",
///     "CBrn" => "Color Burn",
///     "linearBurn" => "Linear Burn",
///     "darkerColor" => "Darken",
///     "Lghn" => "Lighten",
///     "Scrn" => "Screen",
///     "CDdg" => "Color Dodge",
///     "linearDodge" => "Add",
///     "lighterColor" => "Lighten",
///     "Ovrl" => "Overlay",
///     "SftL" => "Soft Light",
///     "HrdL" => "Hard Light",
///     "vividLight" => "Vivid Light",
///     "linearLight" => "Linear Light",
///     "pinLight" => "Pin Light",
///     "hardMix" => "Hard Mix",
///     "Dfrn" => "Difference",
///     "Xclu" => "Exclusion",
///     "blendSubtraction" => "Subtract",
///     "blendDivide" => "Divide",
///     "H   " => "Hue",
///     "Strt" => "Saturation",
///     "Clr " => "Color",
///     "Lmns" => "Luminosity",
///     _ => "Normal",
///```
pub struct BlendModeDocs { _no_init : core::marker::PhantomData<()>, }

#[non_exhaustive]
#[derive(Clone, Debug, Default)]
/// Describes a single layer stack entry.
///
/// This data is very unorganized, and you should not use it directly in your application. You should move it out into your own types.
///
/// Returned from [parse_layer_records].
pub struct LayerInfo {
    /// Name of the layer.
    pub name : String,
    /// Normal opacity of the layer.
    pub opacity : f32,
    /// Photoshop has separate "opacity" and "fill" sliders.
    ///
    /// Fill opacity behaves differently for certain blend modes and when layer effects are involved.
    pub fill_opacity : f32,
    /// Blend mode stored as a string. See [BlendModeDocs].
    pub blend_mode : String,
    /// Global X position of the layer, based on the top left of the canvas. Can be negative. Ignored for groups.
    pub x : i32,
    /// Global Y position of the layer, based on the top left of the canvas. Can be negative. Ignored for groups.
    pub y : i32,
    /// Layer image data width.
    pub w : u32,
    /// Layer image data height.
    pub h : u32,
    /// Number of channels in the image data.
    pub image_channel_count : u16,
    /// Four channels worth of image data. Can be RGBA or CMYA, sometimes with fewer channels. This is non-planar: a single full RGBA pixel is 4 consecutive bytes.
    pub image_data_rgba : Vec<u8>,
    /// The K channel of CMYK image data, if present.
    pub image_data_k : Vec<u8>,
    /// Whether the second channel of the RGBA data came from the PSD file (true) or was synthesized (false).
    ///
    /// If the PSD is malformed and has multiple channels of the same type, this flag might be incorrect.
    /// But on well-formed PSDs, it's always correct.
    pub image_data_has_g : bool,
    /// Whether the third channel of the RGBA data came from the PSD file (true) or was synthesized (false).
    ///
    /// If the PSD is malformed and has multiple channels of the same type, this flag might be incorrect.
    /// But on well-formed PSDs, it's always correct.
    pub image_data_has_b : bool,
    /// Whether the fourth channel of the RGBA data came from the PSD file (true) or was synthesized (false).
    ///
    /// If the PSD is malformed and has multiple channels of the same type, this flag might be incorrect.
    /// But on well-formed PSDs, it's always correct.
    pub image_data_has_a : bool,
    /// Number of channels in the mask image data. They are stored planar (all of ch1, then all of ch2, etc), not interleaved like RGBA.
    pub mask_channel_count : u16,
    /// Where is the mask, and how do you interpret it?
    pub mask_info : MaskInfo,
    // TODO
    //pub global_mask_opacity : u16,
    //pub global_mask_kind : u16,
    /// Actual mask data. Again, this is planar, unlike RGBA.
    pub image_data_mask : Vec<u8>,
    /// If this is a group opener, is the group expanded?
    pub group_expanded : bool,
    /// Is this a group opener?
    pub group_opener : bool,
    /// Is this a group closer?
    pub group_closer : bool,
    /// PSD layers have a "transparency shapes layer" flag. This is the inverse of that flag, i.e. "true" means "the transparency-shapes-layer flag is disabled". This flag state is funny and does weird things to some blend modes and layer effects.
    pub funny_flag : bool,
    /// Does this layer have the "clipping mask" flag enabled?
    pub is_clipped : bool,
    /// Is this layer alpha locked?
    pub is_alpha_locked : bool,
    /// Is this layer visible?
    pub is_visible : bool,
    /// Is this an adjustment layer, and if so, what kind? Blank if not an adjustment layer.
    pub adjustment_type : String,
    /// Pile of raw, flattened adjustment layer metadata. Search `// Read adjustment data.` in the [source code](https://docs.rs/crate/rawpsd/latest/source/src/lib.rs) and read down from there to see how each adjustment's data is flattened.
    pub adjustment_info : Vec<f32>,
    /// Some adjustments use class descriptors instead of "hardcoded" data. Those adjustments get their data here.
    pub adjustment_desc : Option<Descriptor>,
    /// What effects, if any, does this layer have attached to it?
    pub effects_desc : Option<Descriptor>,
}

fn read_u8(cursor: &mut SliceCursor) -> Result<u8, String>
{
    let mut buf = [0; 1];
    cursor.read_exact(&mut buf).map_err(|x| x.to_string())?;
    Ok(buf[0])
}

fn read_u16(cursor: &mut SliceCursor) -> Result<u16, String>
{
    let mut buf = [0; 2];
    cursor.read_exact(&mut buf).map_err(|x| x.to_string())?;
    Ok(u16::from_be_bytes(buf))
}

fn read_u32(cursor: &mut SliceCursor) -> Result<u32, String>
{
    let mut buf = [0; 4];
    cursor.read_exact(&mut buf).map_err(|x| x.to_string())?;
    Ok(u32::from_be_bytes(buf))
}

fn read_b4(cursor: &mut SliceCursor) -> Result<[u8; 4], String>
{
    let mut buf = [0; 4];
    cursor.read_exact(&mut buf).map_err(|x| x.to_string())?;
    Ok(buf)
}

fn read_i32(cursor: &mut SliceCursor) -> Result<i32, String>
{
    let mut buf = [0; 4];
    cursor.read_exact(&mut buf).map_err(|x| x.to_string())?;
    Ok(i32::from_be_bytes(buf))
}

fn read_f64(cursor: &mut SliceCursor) -> Result<f64, String>
{
    let mut buf = [0; 8];
    cursor.read_exact(&mut buf).map_err(|x| x.to_string())?;
    Ok(f64::from_be_bytes(buf))
}

/// Parses just the frontmost metadata at the start of a PSD file.
///
/// You will need to use both this and [parse_layer_records].
pub fn parse_psd_metadata(data : &[u8]) -> Result<PsdMetadata, String>
{
    let mut cursor = SliceCursor::new(data);

    let signature = read_b4(&mut cursor)?;
    if signature != [0x38, 0x42, 0x50, 0x53]
    {
        return Err("Invalid PSD signature".to_string());
    }

    let version = read_u16(&mut cursor)?;
    if version != 1
    {
        return Err("Unsupported PSD version".to_string());
    }

    cursor.set_position(cursor.position() + 6);

    let channel_count = read_u16(&mut cursor)?;
    let height = read_u32(&mut cursor)?;
    let width = read_u32(&mut cursor)?;
    let depth = read_u16(&mut cursor)?;
    let color_mode = read_u16(&mut cursor)?;

    Ok(PsdMetadata
    {
        width,
        height,
        channel_count,
        depth,
        color_mode,
    })
}
/// Decompress a packbits image data buffer into a vec, appending to the vec.
///
/// On success, returns `Ok(size)`.
///
/// Panics if there isn't enough data.
///
/// PSD files generally use compression on their image data. This decompresses it into a vec, bytewise.
pub fn append_img_data(cursor : &[u8], output : &mut Vec<u8>, size : u64, h : u64) -> Result<usize, String>
{
    let mut _cursor = SliceCursor::new(cursor);
    let cursor = &mut _cursor;
    //println!("starting at: {:X}\t", cursor.position());
    let mode = read_u16(cursor)?;
    if mode == 0
    {
        cursor.take(size).read_to_end(output).map_err(|x| x.to_string())?;
    }
    else if mode == 1
    {
        let mut c2 = cursor.clone();
        c2.set_position(c2.position() + h * 2);
        for _ in 0..h
        {
            //println!("at: {:X} - {:X}\t", cursor.position(), c2.position());
            let len = read_u16(cursor)?;
            let start = c2.position();
            // FIXME: ignore overflow and pad out underflow?
            while c2.position() < start + len as u64
            {
                let n = read_u8(&mut c2)? as i8;
                if n >= 0
                {
                    c2.take(n as u64 + 1).read_to_end(output).map_err(|x| x.to_string())?;
                }
                else if n != -128
                {
                    output.extend(core::iter::repeat_n(read_u8(&mut c2)?, (1 - n as i64) as usize));
                }
            }
        }
        cursor.set_position(c2.position());
    }
    else
    {
        return Err("unsupported compression format".to_string());
    }
    Ok(cursor.position() as usize)
}
/// Decompress a packbits image data buffer into a slice, writing into the slice in-place. `stride` can be used to control how far apart to write each byte.
///
/// On success, returns `Ok(size)`.
///
/// Panics if the slice isn't big enough or there isn't enough data.
///
/// PSD files generally use compression on their image data. This decompresses it into a slice, bytewise.
pub fn copy_img_data(cursor : &[u8], output : &mut [u8], stride : usize, size : u64, h : u64) -> Result<usize, String>
{
    let mut _cursor = SliceCursor::new(cursor);
    let cursor = &mut _cursor;
    //println!("pos... 0x{:X}", cursor.position());
    let pos = cursor.position();
    let mode = read_u16(cursor)?;
    //println!("size... 0x{:X}", size as usize - 2);
    if mode == 0
    {
        for i in 0..size as usize - 2
        {
            output[i*stride] = read_u8(cursor)?;
        }
    }
    else if mode == 1
    {
        let mut c2 = cursor.clone();
        c2.set_position(c2.position() + h * 2);
        let mut i = 0;
        let mut j = 2;
        for _ in 0..h
        {
            let _i2 = i;
            //print!("at: {:X} - {:X}\t", cursor.position(), c2.position());
            let len = read_u16(cursor)?;
            j += 2;
            let start = c2.position();
            // FIXME: ignore overflow and pad out underflow?
            while c2.position() - start < len as u64
            {
                let n = read_u8(&mut c2)? as i8;
                j += 1;
                if n >= 0
                {
                    for _ in 0..n as u64 + 1
                    {
                        let c = read_u8(&mut c2)?;
                        if i*stride < output.len()
                        {
                            output[i*stride] = c;
                        }
                        i += 1;
                        j += 1;
                    }
                }
                else if n != -128
                {
                    let c = read_u8(&mut c2)?;
                    for _ in 0..1 - n as i64
                    {
                        if i*stride < output.len()
                        {
                            output[i*stride] = c;
                        }
                        i += 1;
                    }
                    j += 1;
                }
            }
            //println!("effective w: {}", i - _i2);
            c2.set_position(start + len as u64);
        }
        if j != size
        {
            return Err("Desynchronized while reading image data".to_string());
        }
    }
    else
    {
        return Err(format!("unsupported compression format {} at 0x{:X}", mode, pos));
    }
    Ok(size as usize)
}
/// Parses the layer records out of a PSD file, producing a bottom-to-top list.
///
/// PSD data is compressed and poorly-ordered, so it's very rare to benefit from streaming loading, even for performance. Therefore, to keep things simple, the input is a slice instead of a streaming trait.
///
/// PSD doesn't store its layer data in a tree; instead, it uses start-of-group and end-of-group nodes in a list to indicate tree structure.
///
/// On failure, returns all the layers that have been parsed *so far, from the bottom* (PSD files are stored bottom-up), and a string describing the error. The incomplete list of parsed layers is unlikely to be useful for any application, but it may be useful for debugging.
///
/// You will need to use both this and [parse_psd_metadata].
pub fn parse_layer_records(data : &[u8]) -> Result<Vec<LayerInfo>, (Vec<LayerInfo>, String)>
{
    let mut layers = Vec::new();
    let ret = parse_layer_records_impl(data, &mut layers);
    match ret
    {
        Ok(_) => Ok(layers),
        Err(err) => Err((layers, err)),
    }
}
fn parse_layer_records_impl(data : &[u8], layers : &mut Vec<LayerInfo>) -> Result<(), String>
{
    let metadata = parse_psd_metadata(data)?;
    if metadata.depth != 8
    {
        return Err("Only PSDs in 8-bit RGB, CMYK, or Grayscale mode are currently supported.".to_string());
    }
    // TODO
    if metadata.color_mode != 1 && metadata.color_mode != 3 && metadata.color_mode != 4
    {
        return Err("Only PSDs in 8-bit RGB, CMYK, or Grayscale mode are currently supported.".to_string());
    }
    
    let mut cursor = SliceCursor::new(data);
    cursor.set_position(26);

    let color_mode_length = read_u32(&mut cursor)? as u64;
    cursor.set_position(cursor.position() + color_mode_length);

    let image_resources_length = read_u32(&mut cursor)? as u64;
    cursor.set_position(cursor.position() + image_resources_length);

    let layer_mask_info_length = read_u32(&mut cursor)? as u64;
    let _layer_mask_info_end = cursor.position() + layer_mask_info_length;

    let layer_info_length = read_u32(&mut cursor)? as u64;
    let _layer_info_end = cursor.position() + layer_info_length;
    
    let layer_count = read_u16(&mut cursor)? as i16;
    let layer_count = layer_count.abs(); // If negative, transparency info exists
    
    #[cfg(feature = "debug_spew")]
    println!("starting at {:X}", cursor.position());
    
    let mut idata_c = SliceCursor::new(data);
    idata_c.set_position(cursor.position());
    
    for _i in 0..layer_count
    {
        //println!("{}", _i);
        read_i32(&mut idata_c)?;
        read_i32(&mut idata_c)?;
        read_i32(&mut idata_c)?;
        read_i32(&mut idata_c)?;
        let image_channel_count = read_u16(&mut idata_c)? as u64;
        idata_c.set_position(idata_c.position() + 6*image_channel_count + 4 + 4 + 4);
        let idat_len = read_u32(&mut idata_c)? as u64;
        idata_c.set_position(idata_c.position() + idat_len);
    }

    for _ in 0..layer_count
    {
        let top = read_i32(&mut cursor)?;
        let left = read_i32(&mut cursor)?;
        let bottom = read_i32(&mut cursor)?;
        let right = read_i32(&mut cursor)?;

        let x = left;
        let y = top;
        let w = (right - left) as u32;
        let h = (bottom - top) as u32;
        
        let image_channel_count = read_u16(&mut cursor)?;
        //println!("chan count {}", image_channel_count);
        
        let channel_info_start = cursor.position();
        
        cursor.set_position(channel_info_start);
        let mut image_data_rgba : Vec<u8> = vec![255u8; (w * h * 4) as usize];
        let mut image_data_k : Vec<u8> = vec!();
        let mut image_data_mask : Vec<u8> = vec!();
        
        let mut _rgba_count = 0;
        let mut has_g = false;
        let mut has_b = false;
        let mut has_a = false;
        let mut aux_count = 0;
        
        let mut cdat_cursor = cursor.clone();
        
        let mut has_neg2 = false;
        let mut has_neg3 = false;
        for _ in 0..image_channel_count
        {
            let channel_id = read_u16(&mut cursor)? as i16;
            let _channel_length = read_u32(&mut cursor)? as usize;
            has_neg2 = has_neg2 || channel_id == -2;
            has_neg3 = has_neg3 || channel_id == -3;
        }
        
        let blend_mode_signature = read_b4(&mut cursor)?;
        if blend_mode_signature != [0x38, 0x42, 0x49, 0x4D]
        {
            return Err("Invalid blend mode magic signature".to_string());
        }

        let blend_mode_key = read_b4(&mut cursor)?;
        let blend_mode = String::from_utf8_lossy(&blend_mode_key).to_string();

        let opacity = read_u8(&mut cursor)? as f32 / 255.0;
        #[cfg(feature = "debug_spew")]
        println!("opacity: {}", opacity * 100.0);
        let clipping = read_u8(&mut cursor)?;
        let flags = read_u8(&mut cursor)?;
        let _filler = read_u8(&mut cursor)?;

        let exdat_len = read_u32(&mut cursor)? as u64;
        let exdat_start = cursor.position();
        
        let maskdat_len = read_u32(&mut cursor)? as u64;
        let maskdat_start = cursor.position();
        
        // FIXME: support maskdat_len == 0 case
        let mtop = read_i32(&mut cursor)?;
        let mleft = read_i32(&mut cursor)?;
        let mbottom = read_i32(&mut cursor)?;
        let mright = read_i32(&mut cursor)?;
        let mut mask_info = MaskInfo::default();
        mask_info.x = mleft;
        mask_info.y = mtop;
        mask_info.w = (mright - mleft) as u32;
        mask_info.h = (mbottom - mtop) as u32;
        mask_info.default_color = read_u8(&mut cursor)?;
        let mflags = read_u8(&mut cursor)?;
        mask_info.relative = (mflags & 1) != 0;
        mask_info.disabled = (mflags & 2) != 0;
        mask_info.invert = (mflags & 4) != 0;
        
        cursor.set_position(maskdat_start + maskdat_len);
        
        for _ in 0..image_channel_count
        {
            let channel_id = read_u16(&mut cdat_cursor)? as i16;
            has_g |= channel_id == 1;
            has_b |= channel_id == 2;
            has_a |= channel_id == -1;
            let channel_length = read_u32(&mut cdat_cursor)? as usize;
            #[cfg(feature = "debug_spew")]
            println!("channel... {} {} at 0x{:X}", channel_id, channel_length, idata_c.position());
            if channel_id >= -1 && channel_id <= 2
            {
                _rgba_count += 1;
                let pos = if channel_id >= 0 { channel_id } else { 3 } as usize;
                #[cfg(feature = "debug_spew")]
                println!("{} {} {} {}", w, h, pos, channel_length);
                if channel_length > 2
                {
                    let progress = copy_img_data(idata_c.take_rest().buf, &mut image_data_rgba[pos..], 4, channel_length as u64, h as u64)?;
                    idata_c.pos += progress;
                }
                else
                {
                    idata_c.set_position(idata_c.position() + 2);
                }
            }
            else if channel_id == 3 // CMYK's K
            {
                if channel_length > 2
                {
                    let progress = append_img_data(idata_c.take_rest().buf, &mut image_data_k, channel_length as u64, h as u64)?;
                    idata_c.pos += progress;
                }
                else
                {
                    idata_c.set_position(idata_c.position() + 2);
                }
            }
            else
            {
                #[cfg(feature = "debug_spew")]
                println!("mask... {} {} {}", mask_info.w, mask_info.h, channel_length);
                aux_count += 1;
                if aux_count > 1
                {
                    idata_c.set_position(idata_c.position() + channel_length as u64);
                }
                else if channel_length > 2
                {
                    #[cfg(feature = "debug_spew")]
                    println!("adding mask data...");
                    let progress = append_img_data(idata_c.take_rest().buf, &mut image_data_mask, channel_length as u64, mask_info.h as u64)?;
                    idata_c.pos += progress;
                }
                else
                {
                    idata_c.set_position(idata_c.position() + 2);
                }
            }
        }
        
        let blendat_len = read_u32(&mut cursor)? as u64;
        cursor.set_position(cursor.position() + blendat_len);
        
        let mut name_len = read_u8(&mut cursor)?;
        let orig_namelen = name_len;
        while (name_len + 1) % 4 != 0
        {
            name_len += 1;
        }
        let mut name = vec![0; name_len as usize];
        cursor.read_exact(&mut name[..]).map_err(|x| x.to_string())?;
        let name = String::from_utf8_lossy(&name[..orig_namelen as usize]).to_string();

        let mut layer = LayerInfo {
            name,
            opacity,
            fill_opacity : 1.0,
            blend_mode,
            x,
            y,
            w,
            h,
            image_channel_count,
            image_data_rgba,
            image_data_k,
            image_data_has_g : has_g,
            image_data_has_b : has_b,
            image_data_has_a : has_a,
            mask_channel_count : aux_count,
            mask_info,
            image_data_mask,
            group_expanded : false,
            group_opener : false,
            group_closer : false,
            funny_flag : false,
            is_clipped : clipping != 0,
            is_alpha_locked : (flags & 1) != 0,
            is_visible : (flags & 2) == 0,
            adjustment_type : "".to_string(),
            adjustment_info : vec!(),
            adjustment_desc : None,
            effects_desc : None,
        };
        
        //println!("--- {:X}", cursor.position());
        
        while cursor.position() < exdat_start + exdat_len
        {
            let sig = read_b4(&mut cursor)?;
            if sig != [0x38, 0x42, 0x49, 0x4D]
            {
                return Err("Invalid extended data magic signature".to_string());
            }
            
            let name = read_b4(&mut cursor)?;
            let name = String::from_utf8_lossy(&name).to_string();
            
            let len = read_u32(&mut cursor)? as u64;
            //println!("?? {}", len);
            let start = cursor.position();
            
            #[cfg(feature = "debug_spew")]
            println!("reading metadata.... {}", name.as_str());
            
            fn read_descriptor(c : &mut SliceCursor) -> Result<Descriptor, String>
            {
                // skip name. usually/often blank
                let n = read_u32(c)? as u64;
                c.set_position(c.position() + n * 2);
                
                let mut idlen = read_u32(c)?;
                if idlen == 0 { idlen = 4; }
                let mut id = vec![0; idlen as usize];
                c.read_exact(&mut id).map_err(|x| x.to_string())?;
                let id = String::from_utf8_lossy(&id).to_string();
                
                let mut data = vec!();
                
                let itemcount = read_u32(c)?;
                
                for _ in 0..itemcount
                {
                    let mut namelen = read_u32(c)?;
                    if namelen == 0 { namelen = 4; }
                    let mut name = vec![0; namelen as usize];
                    c.read_exact(&mut name).map_err(|x| x.to_string())?;
                    let name = String::from_utf8_lossy(&name).to_string();
                    
                    fn read_key(c : &mut SliceCursor) -> Result<DescItem, String>
                    {
                        let id = read_b4(c)?;
                        let id = String::from_utf8_lossy(&id).to_string();
                        
                        Ok(match id.as_str()
                        {
                            "long" => DescItem::long(read_i32(c)?),
                            "doub" => DescItem::doub(read_f64(c)?),
                            "Objc" => DescItem::Objc(Box::new(read_descriptor(c)?)),
                            "bool" => DescItem::bool(read_u8(c)? != 0),
                            "TEXT" =>
                            {
                                let len = read_u32(c)? as u64;
                                let mut text = vec![0; len as usize];
                                for i in 0..len
                                {
                                    text[i as usize] = read_u16(c)?;
                                }
                                let text = String::from_utf16_lossy(&text).trim_end_matches('\0').to_string();
                                DescItem::TEXT(text)
                            }
                            "UntF" =>
                            {
                                let typ = read_b4(c)?;
                                let typ = String::from_utf8_lossy(&typ).to_string();
                                
                                DescItem::UntF(typ, read_f64(c)?)
                            }
                            "enum" =>
                            {
                                let mut len = read_u32(c)?;
                                if len == 0 { len = 4; }
                                let mut name1 = vec![0; len as usize];
                                c.read_exact(&mut name1).map_err(|x| x.to_string())?;
                                let name1 = String::from_utf8_lossy(&name1).to_string();
                                
                                let mut len = read_u32(c)?;
                                if len == 0 { len = 4; }
                                let mut name2 = vec![0; len as usize];
                                c.read_exact(&mut name2).map_err(|x| x.to_string())?;
                                let name2 = String::from_utf8_lossy(&name2).to_string();
                                
                                DescItem::_enum(name1, name2)
                            }
                            "VlLs" =>
                            {
                                let len = read_u32(c)?;
                                let mut ret = vec!();
                                for _ in 0..len
                                {
                                    ret.push(read_key(c)?);
                                }
                                DescItem::VlLs(ret)
                            }
                            _ =>
                            {
                                #[cfg(feature = "debug_spew")]
                                println!("!!! errant descriptor subobject type... {}", id);
                                DescItem::Err(format!("!!! errant descriptor subobject type... {}", id))
                            }
                        })
                    }
                    
                    data.push((name, read_key(c)?));
                }
                
                Ok((id, data))
            }
            
            // This comment must stay here: it is a ctrl+f anchor.
            // Read adjustment data.
            match name.as_str()
            {
                "lsct" =>
                {
                    let kind = read_u32(&mut cursor)? as u64;
                    layer.group_expanded = kind == 1;
                    layer.group_opener = kind == 1 || kind == 2;
                    layer.group_closer = kind == 3;
                    if kind == 1 || kind == 2
                    {
                        #[cfg(feature = "debug_spew")]
                        println!("group opener!");
                    }
                    if kind == 3
                    {
                        #[cfg(feature = "debug_spew")]
                        println!("group closer!");
                    }
                }
                "luni" =>
                {
                    let len = read_u32(&mut cursor)? as u64;
                    let mut name = vec![0; len as usize];
                    for i in 0..len
                    {
                        name[i as usize] = read_u16(&mut cursor)?;
                    }
                    layer.name = String::from_utf16_lossy(&name).to_string();
                }
                "tsly" =>
                {
                    let thing = read_u8(&mut cursor)?;
                    layer.funny_flag = thing == 0;
                    #[cfg(feature = "debug_spew")]
                    println!("{}", layer.funny_flag);
                }
                "iOpa" =>
                {
                    layer.fill_opacity = read_u8(&mut cursor)? as f32 / 255.0;
                }
                "lfx2" =>
                {
                    if read_u32(&mut cursor)? == 0 && read_u32(&mut cursor)? == 16
                    {
                        layer.effects_desc = Some(read_descriptor(&mut cursor)?);
                    }
                    else
                    {
                        read_descriptor(&mut cursor)?;
                    }
                }
                // adjustment layers
                "post" =>
                {
                    let mut data = vec!();
                    data.push(read_u16(&mut cursor)? as f32); // number of levels
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = data;
                }
                "nvrt" =>
                {
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = vec!();
                }
                "brit" =>
                {
                    let mut data = vec!();
                    data.push(read_u16(&mut cursor)? as f32); // brightness
                    data.push(read_u16(&mut cursor)? as f32); // contrast
                    data.push(read_u16(&mut cursor)? as f32); // "Mean value for brightness and contrast"
                    data.push(read_u8(&mut cursor)? as f32); // "Lab color only"
                    data.push(1.0); // legacy mode
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = data;
                }
                "thrs" =>
                {
                    let mut data = vec!();
                    data.push(read_u16(&mut cursor)? as f32);
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = data;
                }
                "hue2" =>
                {
                    let mut data = vec!();
                    
                    read_u16(&mut cursor)?; // version
                    data.push(read_u8(&mut cursor)? as f32); // if 1, is absolute/colorization (rather than relative)
                    read_u8(&mut cursor)?;
                    
                    // "colorization"
                    data.push(read_u16(&mut cursor)? as i16 as f32); // hue
                    data.push(read_u16(&mut cursor)? as i16 as f32); // sat
                    data.push(read_u16(&mut cursor)? as i16 as f32); // lightness (-1 to +1)
                    
                    // "master"
                    data.push(read_u16(&mut cursor)? as i16 as f32); // hue
                    data.push(read_u16(&mut cursor)? as i16 as f32); // sat
                    data.push(read_u16(&mut cursor)? as i16 as f32); // lightness (-1 to +1)
                    
                    // todo: read hextant values?
                    
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = data;
                }
                "levl" =>
                {
                    let mut data = vec!();
                    
                    if read_u16(&mut cursor)? != 2
                    {
                        return Err("Ran into an unsupported subdata version".to_string());
                    }
                    for _ in 0..28
                    {
                        data.push(read_u16(&mut cursor)? as f32 / 255.0); // in floor
                        data.push(read_u16(&mut cursor)? as f32 / 255.0); // in ceil
                        data.push(read_u16(&mut cursor)? as f32 / 255.0); // out floor
                        data.push(read_u16(&mut cursor)? as f32 / 255.0); // out ceil
                        data.push(read_u16(&mut cursor)? as f32 / 100.0); // gamma
                    }
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = data;
                }
                "curv" =>
                {
                    let mut data = vec!();
                    
                    read_u8(&mut cursor)?;
                    if read_u16(&mut cursor)? != 1
                    {
                        return Err("Ran into an unsupported subdata version".to_string());
                    }
                    let enabled = read_u32(&mut cursor)?;
                    
                    for i in 0..32
                    {
                        if (enabled & (1u32 << i)) != 0
                        {
                            let n = read_u16(&mut cursor)?;
                            data.push(n as f32); // number of points
                            for _ in 0..n
                            {
                                let y = read_u16(&mut cursor)? as f32 / 255.0;
                                data.push(read_u16(&mut cursor)? as f32 / 255.0); // x
                                data.push(y); // y
                            }
                        }
                        else
                        {
                            data.push(0.0); // number of points
                        }
                    }
                    layer.adjustment_type = name.clone();
                    layer.adjustment_info = data;
                }
                "blwh" =>
                {
                    if read_u32(&mut cursor)? != 16
                    {
                        return Err("Ran into an unsupported subdata version".to_string());
                    }
                    layer.adjustment_type = name.clone();
                    layer.adjustment_desc = Some(read_descriptor(&mut cursor)?);
                }
                "CgEd" =>
                {
                    if read_u32(&mut cursor)? != 16
                    {
                        return Err("Ran into an unsupported subdata version".to_string());
                    }
                    //layer.adjustment_type = name.clone();
                    //layer.adjustment_type = "brit".to_string();
                    let temp = read_descriptor(&mut cursor)?.1;
                    #[cfg(feature = "debug_spew")]
                    println!("{:?}", temp);
                    let mut n = BTreeMap::new();
                    for t in temp
                    {
                        n.insert(t.0, t.1);
                    }
                    #[cfg(feature = "debug_spew")]
                    println!("{:?}", n);
                    //("null", [("Vrsn", long(1)), ("Brgh", long(9)), ("Cntr", long(30)), ("means", long(127)), ("Lab ", bool(false)), ("useLegacy", bool(true)), ("Auto", bool(true))])
                    let mut data = vec!();
                    data.push(n.get("Brgh").ok_or("Malformed data structure".to_string())?.long() as f32);
                    data.push(n.get("Cntr").ok_or("Malformed data structure".to_string())?.long() as f32);
                    data.push(n.get("means").ok_or("Malformed data structure".to_string())?.long() as f32);
                    data.push(n.get("Lab ").ok_or("Malformed data structure".to_string())?.bool() as u8 as f32);
                    data.push(n.get("useLegacy").ok_or("Malformed data structure".to_string())?.bool() as u8 as f32);
                    #[cfg(feature = "debug_spew")]
                    println!("??????????? {:?}", data);
                    layer.adjustment_info = data;
                }
                _ => {}
            }
            cursor.set_position(start + len);
        }
        //println!("{:X} {:X}", cursor.position(), exdat_start + exdat_len);
        if cursor.position() != exdat_start + exdat_len
        {
            return Err("Desynchronized while reading or skipping extra data".to_string());
        }
        
        #[cfg(feature = "debug_spew")]
        println!("added layer with name {}", layer.name);
        layers.push(layer);
    }
    
    Ok(())
}

#[non_exhaustive]
#[derive(Debug, PartialEq)]
/// File-wide PSD header metadata.
///
/// Returned from [parse_psd_metadata].
pub struct PsdMetadata {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// PSD-wide color mode constant. See <https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/#50577409_pgfId-1055726>
    pub color_mode: u16,
    /// Color depth in bytes. Only 8-bit (1-byte) images are currently supported.
    pub depth: u16,
    /// Number of channels in the PSD file's colorspace, including alpha. Only Y/YA, RGB/RGBA, and CMYK/CMYKA images are currently supported.
    pub channel_count: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test()
    {
        let data = std::fs::read("data/test.psd").expect("Failed to open test.psd");

        if let Ok(layers) = parse_layer_records(&data)
        {
            for mut layer in layers
            {
                // Don't spew tons of image data bytes to stdout; we just want to see the metadata.
                layer.image_data_rgba = vec!();
                layer.image_data_k = vec!();
                layer.image_data_mask = vec!();
                println!("{:?}", layer);
            }
        }
        
        println!("-----");
        
        let data = std::fs::read("data/test2.psd").expect("Failed to open test2.psd");

        if let Ok(layers) = parse_layer_records(&data)
        {
            for mut layer in layers
            {
                layer.image_data_rgba = vec!();
                layer.image_data_k = vec!();
                layer.image_data_mask = vec!();
                println!("{:?}", layer);
            }
        }
    }
}
