//! NBT primitives shared by the file codecs. SNBT keeps numeric tag types visible.
use crate::{
    Result,
    model::{Compound, Position},
};
pub use fastnbt::Value as Tag;
use fastnbt::{ByteArray, IntArray, LongArray, Value};
use std::io::{Read, Write};
/// Maximum encoded or expanded NBT input size in bytes.
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
/// Borrows a compound tag, or returns an error for another tag type.
pub fn compound(v: &Value) -> Result<&Compound> {
    if let Value::Compound(c) = v {
        Ok(c)
    } else {
        Err("Expected NBT compound".into())
    }
}
/// Borrows a list tag, or returns an error for another tag type.
pub fn list(v: &Value) -> Result<&Vec<Value>> {
    if let Value::List(c) = v {
        Ok(c)
    } else {
        Err("Expected NBT list".into())
    }
}
/// Borrows a named field, or returns an error if it is absent.
pub fn get<'a>(c: &'a Compound, k: &str) -> Result<&'a Value> {
    c.get(k).ok_or_else(|| format!("Missing NBT field {k}"))
}
/// Reads an integer tag as i32, rejecting other types and overflow.
pub fn number(v: &Value) -> Result<i32> {
    match v {
        Value::Byte(n) => Ok(*n as i32),
        Value::Short(n) => Ok(*n as i32),
        Value::Int(n) => Ok(*n),
        Value::Long(n) => i32::try_from(*n).map_err(|_| "Integer out of range".into()),
        _ => Err("Expected integer NBT".into()),
    }
}
/// Borrows a string tag, or returns an error for another tag type.
pub fn string(v: &Value) -> Result<&str> {
    if let Value::String(s) = v {
        Ok(s)
    } else {
        Err("Expected string NBT".into())
    }
}
/// Reads three integer coordinates from an int array, list, or x/y/z compound.
pub fn xyz(v: &Value) -> Result<Position> {
    match v {
        Value::IntArray(a) if a.len() == 3 => Ok([a[0], a[1], a[2]]),
        Value::List(l) if l.len() == 3 => Ok([number(&l[0])?, number(&l[1])?, number(&l[2])?]),
        Value::Compound(c) => Ok([
            number(get(c, "x")?)?,
            number(get(c, "y")?)?,
            number(get(c, "z")?)?,
        ]),
        _ => Err("Expected three integer coordinates".into()),
    }
}
/// Reads a three-element float or double list, rejecting nonfinite coordinates.
pub fn doubles(v: &Value) -> Result<[f64; 3]> {
    let l = list(v)?;
    if l.len() != 3 {
        return Err("Expected three entity coordinates".into());
    }
    let mut p = [0.; 3];
    for i in 0..3 {
        p[i] = match l[i] {
            Value::Double(v) => v,
            Value::Float(v) => v as f64,
            _ => return Err("Expected floating point entity coordinates".into()),
        };
        if !p[i].is_finite() {
            return Err("Entity position must be finite".into());
        }
    }
    Ok(p)
}
/// Encodes X, Y, and Z as a list of integer tags.
pub fn ints(p: Position) -> Value {
    Value::List(p.into_iter().map(Value::Int).collect())
}
/// Encodes X, Y, and Z as an NBT integer array.
pub fn int_array(p: Position) -> Value {
    Value::IntArray(IntArray::new(p.to_vec()))
}
/// Encodes integer coordinates as an x/y/z compound.
pub fn pos_compound(p: Position) -> Value {
    Value::Compound(
        ["x", "y", "z"]
            .into_iter()
            .zip(p)
            .map(|(k, v)| (k.into(), Value::Int(v)))
            .collect(),
    )
}
/// Encodes X, Y, and Z as a list of double tags.
pub fn double_list(p: [f64; 3]) -> Value {
    Value::List(p.into_iter().map(Value::Double).collect())
}
/// Creates a compound tag from named fields.
pub fn c<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Compound(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
/// Creates a string tag.
pub fn s(s: impl Into<String>) -> Value {
    Value::String(s.into())
}
/// Parses a typed SNBT compound without erasing numeric tag types.
pub fn from_snbt(s: &str) -> Result<Compound> {
    parse_snbt(s)
}

pub(crate) fn parse_snbt<T: serde::de::DeserializeOwned>(input: &str) -> Result<T> {
    fn finish_token(output: &mut String, token: &mut Option<usize>, key: bool) {
        if let Some(start) = token.take() {
            if key {
                let field = output[start..].to_string();
                output.truncate(start);
                output.push('"');
                output.push_str(&field);
                output.push('"');
                return;
            }
            let replacement = match &output[start..] {
                "true" => Some("1b"),
                "false" => Some("0b"),
                _ => None,
            };
            if let Some(replacement) = replacement {
                output.truncate(start);
                output.push_str(replacement);
            }
        }
    }
    let mut output = String::with_capacity(input.len());
    let mut quote = None;
    let mut escaped = false;
    let mut separated = false;
    let mut previous = None;
    let mut token = None;
    for ch in input.chars() {
        if let Some(delimiter) = quote {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
        } else if ch.is_whitespace() {
            separated = true;
            continue;
        } else {
            let structural = |ch| matches!(ch, '{' | '}' | '[' | ']' | ':' | ',' | ';');
            if separated && previous.is_some_and(|last| !structural(last) && !structural(ch)) {
                return Err("SNBT: missing delimiter between tokens".into());
            }
            if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '+') {
                token.get_or_insert(output.len());
            } else {
                finish_token(&mut output, &mut token, ch == ':');
            }
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            }
            output.push(ch);
        }
        previous = Some(ch);
        separated = false;
    }
    finish_token(&mut output, &mut token, false);
    fastsnbt::from_str(&output).map_err(|e| e.to_string())
}
/// Serializes a compound as typed SNBT.
pub fn to_snbt(c: &Compound) -> Result<String> {
    fastsnbt::to_string(c).map_err(|e| e.to_string())
}

pub(crate) fn snbt_lists(data: &mut Compound, binary: bool) -> Result<()> {
    fn walk(value: &mut Value, binary: bool, depth: usize) -> Result<()> {
        if depth > 512 {
            return Err("SNBT exceeds maximum nesting depth".into());
        }
        match value {
            Value::Compound(fields) => {
                for value in fields.values_mut() {
                    walk(value, binary, depth + 1)?;
                }
            }
            Value::List(values) => {
                for value in values.iter_mut() {
                    if !binary
                        && let Value::Compound(fields) = value
                        && fields.len() == 1
                        && let Some(inner) = fields.remove("")
                    {
                        *value = inner;
                    }
                    walk(value, binary, depth + 1)?;
                }
                if binary {
                    let mixed = values.first().is_some_and(|first| {
                        values.iter().any(|value| {
                            std::mem::discriminant(value) != std::mem::discriminant(first)
                        })
                    });
                    for value in values {
                        let marker = matches!(value, Value::Compound(fields) if fields.len() == 1 && fields.contains_key(""));
                        if marker || (mixed && !matches!(value, Value::Compound(_))) {
                            let inner = std::mem::replace(value, Value::Byte(0));
                            *value = Value::Compound(Compound::from([("".into(), inner)]));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    for value in data.values_mut() {
        walk(value, binary, 0)?;
    }
    Ok(())
}
/// Decodes a compound from raw or gzip-compressed NBT.
///
/// little selects Bedrock little-endian encoding. Input and expanded data are size-limited.
pub fn decode(data: &[u8], little: bool) -> Result<Compound> {
    if data.len() > MAX_BYTES {
        return Err("NBT input exceeds 256 MiB".into());
    }
    let raw = if data.starts_with(&[31, 139]) {
        let mut out = vec![];
        flate2::read::GzDecoder::new(data)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        out
    } else {
        data.to_vec()
    };
    if raw.len() > MAX_BYTES {
        return Err("Expanded NBT exceeds 256 MiB".into());
    }
    if !little {
        return fastnbt::from_bytes(&raw).map_err(|e| e.to_string());
    }
    let mut r = Reader { data: &raw, at: 0 };
    if r.byte()? != 10 {
        return Err("NBT root must be a compound".into());
    }
    r.string()?;
    let root = r.payload(10, 0)?;
    if r.at != raw.len() {
        return Err("Trailing NBT bytes".into());
    }
    if let Value::Compound(c) = root {
        Ok(c)
    } else {
        unreachable!()
    }
}
/// Encodes a compound as NBT; little selects Bedrock byte order and gzip compresses it.
pub fn encode(root: &Compound, little: bool, gzip: bool) -> Result<Vec<u8>> {
    let raw = if little {
        let mut w = vec![10, 0, 0];
        write_value(&Value::Compound(root.clone()), &mut w)?;
        w
    } else {
        fastnbt::to_bytes(root).map_err(|e| e.to_string())?
    };
    if !gzip {
        return Ok(raw);
    }
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(&raw).map_err(|e| e.to_string())?;
    e.finish().map_err(|e| e.to_string())
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).ok_or("NBT length overflow")?;
        if end > self.data.len() {
            return Err("Truncated NBT".into());
        }
        let v = &self.data[self.at..end];
        self.at = end;
        Ok(v)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn short(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn int(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn long(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn length(&mut self) -> Result<usize> {
        let n = self.int()?;
        if n < 0 || n as usize > MAX_BYTES {
            return Err("Invalid NBT length".into());
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> Result<String> {
        let n = self.short()? as u16 as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }
    fn payload(&mut self, id: u8, depth: usize) -> Result<Value> {
        if depth > 128 {
            return Err("NBT nesting exceeds 128".into());
        }
        Ok(match id {
            1 => Value::Byte(self.byte()? as i8),
            2 => Value::Short(self.short()?),
            3 => Value::Int(self.int()?),
            4 => Value::Long(self.long()?),
            5 => Value::Float(f32::from_bits(self.int()? as u32)),
            6 => Value::Double(f64::from_bits(self.long()? as u64)),
            7 => {
                let n = self.length()?;
                Value::ByteArray(ByteArray::new(
                    self.take(n)?.iter().map(|&v| v as i8).collect(),
                ))
            }
            8 => Value::String(self.string()?),
            9 => {
                let t = self.byte()?;
                let n = self.length()?;
                if n > self.data.len() - self.at {
                    return Err("List length exceeds remaining input".into());
                }
                let mut l = Vec::new();
                for _ in 0..n {
                    l.push(self.payload(t, depth + 1)?);
                }
                Value::List(l)
            }
            10 => {
                let mut c = Compound::new();
                loop {
                    let t = self.byte()?;
                    if t == 0 {
                        break;
                    }
                    let key = self.string()?;
                    let v = self.payload(t, depth + 1)?;
                    if c.insert(key, v).is_some() {
                        return Err("Duplicate NBT compound key".into());
                    }
                }
                Value::Compound(c)
            }
            11 => {
                let n = self.length()?;
                if n > (self.data.len() - self.at) / 4 {
                    return Err("Truncated int array".into());
                }
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(self.int()?);
                }
                Value::IntArray(IntArray::new(v))
            }
            12 => {
                let n = self.length()?;
                if n > (self.data.len() - self.at) / 8 {
                    return Err("Truncated long array".into());
                }
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(self.long()?);
                }
                Value::LongArray(LongArray::new(v))
            }
            _ => return Err(format!("Unknown NBT tag {id}")),
        })
    }
}
fn tag(v: &Value) -> u8 {
    match v {
        Value::Byte(_) => 1,
        Value::Short(_) => 2,
        Value::Int(_) => 3,
        Value::Long(_) => 4,
        Value::Float(_) => 5,
        Value::Double(_) => 6,
        Value::ByteArray(_) => 7,
        Value::String(_) => 8,
        Value::List(_) => 9,
        Value::Compound(_) => 10,
        Value::IntArray(_) => 11,
        Value::LongArray(_) => 12,
    }
}
fn write_string(s: &str, w: &mut Vec<u8>) -> Result<()> {
    let n = u16::try_from(s.len()).map_err(|_| "NBT string too long")?;
    w.extend(n.to_le_bytes());
    w.extend(s.as_bytes());
    Ok(())
}
fn write_len(n: usize, w: &mut Vec<u8>) -> Result<()> {
    w.extend(
        i32::try_from(n)
            .map_err(|_| "NBT array too long")?
            .to_le_bytes(),
    );
    Ok(())
}
fn write_value(v: &Value, w: &mut Vec<u8>) -> Result<()> {
    match v {
        Value::Byte(n) => w.push(*n as u8),
        Value::Short(n) => w.extend(n.to_le_bytes()),
        Value::Int(n) => w.extend(n.to_le_bytes()),
        Value::Long(n) => w.extend(n.to_le_bytes()),
        Value::Float(n) => w.extend(n.to_le_bytes()),
        Value::Double(n) => w.extend(n.to_le_bytes()),
        Value::String(s) => write_string(s, w)?,
        Value::ByteArray(a) => {
            write_len(a.len(), w)?;
            w.extend(a.iter().map(|&v| v as u8));
        }
        Value::IntArray(a) => {
            write_len(a.len(), w)?;
            for n in a.iter() {
                w.extend(n.to_le_bytes());
            }
        }
        Value::LongArray(a) => {
            write_len(a.len(), w)?;
            for n in a.iter() {
                w.extend(n.to_le_bytes());
            }
        }
        Value::List(l) => {
            let t = l.first().map(tag).unwrap_or(0);
            w.push(t);
            write_len(l.len(), w)?;
            for v in l {
                if tag(v) != t {
                    return Err("NBT lists must have one tag type".into());
                }
                write_value(v, w)?;
            }
        }
        Value::Compound(c) => {
            let mut keys: Vec<_> = c.keys().collect();
            keys.sort();
            for k in keys {
                w.push(tag(&c[k]));
                write_string(k, w)?;
                write_value(&c[k], w)?;
            }
            w.push(0);
        }
    }
    Ok(())
}
