#[derive(Debug)]
pub enum Value {
    Bool(bool),
    Nil,
    PositiveInt(u64),
    NegativeInt(i64),
    Float32(f32),
    Float64(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<Value>),
    Dict(Vec<(Value, Value)>), // Changed from HashMap to Vec of pairs
    Tag(u8, Box<Value>),
    Cstor0(u8),
    Cstor1(u8, Box<Value>),
    CstorN(u8, Vec<Value>),
}

fn decode_leb128(data: &[u8], offset: &mut usize) -> u64 {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let byte = data[*offset];
        *offset += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

fn decode_value(data: &[u8], offset: &mut usize) -> Value {
    let offset0 = *offset;
    let first_byte = data[offset0];

    let kind = (first_byte >> 4) & 0x0F;
    let low = first_byte & 0x0F;
    dbg!(*offset, kind, low);

    *offset += 1;

    match kind {
        0 => match low {
            0 => Value::Bool(false),
            1 => Value::Bool(true),
            2 => Value::Nil,
            _ => unimplemented!(), // Reserved values
        },
        1 => {
            let mut n = low as u64;
            if n == 15 {
                n += decode_leb128(data, offset);
            }
            Value::PositiveInt(n)
        }
        2 => {
            let mut n = low as i64;
            if n == 15 {
                n += decode_leb128(data, offset) as i64;
            }
            Value::NegativeInt(-n - 1)
        }
        3 => match low {
            0 => {
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(&data[*offset..*offset + 4]);
                *offset += 4;
                Value::Float32(f32::from_le_bytes(bytes))
            }
            1 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[*offset..*offset + 8]);
                *offset += 8;
                Value::Float64(f64::from_le_bytes(bytes))
            }
            _ => unimplemented!(), // Reserved values
        },
        4 => {
            let mut len = low as usize;
            if len == 15 {
                len += decode_leb128(data, offset) as usize;
            }
            let s = String::from_utf8(data[*offset..*offset + len].to_vec()).unwrap();
            *offset += len;
            Value::String(s)
        }
        5 => {
            let mut len = low as usize;
            if len == 15 {
                len += decode_leb128(data, offset) as usize;
            }
            let bytes = data[*offset..*offset + len].to_vec();
            *offset += len;
            Value::Binary(bytes)
        }
        6 => {
            let mut count = low as usize;
            if count == 15 {
                count += decode_leb128(data, offset) as usize;
            }
            let mut elements = Vec::with_capacity(count);
            for _ in 0..count {
                elements.push(decode_value(data, offset));
            }
            Value::Array(elements)
        }
        7 => {
            let mut count = low as usize;
            if count == 15 {
                count += decode_leb128(data, offset) as usize;
            }
            let mut pairs = Vec::with_capacity(count);
            for _ in 0..count {
                let key = decode_value(data, offset);
                let value = decode_value(data, offset);
                pairs.push((key, value));
            }
            Value::Dict(pairs)
        }
        8 => {
            let tag = low;
            let value = decode_value(data, offset);
            Value::Tag(tag, Box::new(value))
        }
        10 => Value::Cstor0(low),
        11 => {
            let arg = decode_value(data, offset);
            Value::Cstor1(low, Box::new(arg))
        }
        12 => {
            let mut len = low as usize;
            if len == 15 {
                len += decode_leb128(data, offset) as usize;
            }
            let mut elements = Vec::with_capacity(len);
            for _ in 0..len {
                elements.push(decode_value(data, offset));
            }
            Value::CstorN(low, elements)
        }
        15 => {
            // For pointers, calculate target offset and decode the value there
            let mut relative_offset = low as usize;
            if relative_offset == 15 {
                relative_offset += decode_leb128(data, offset) as usize;
            }
            let target_offset = offset0 - relative_offset - 1;
            let mut new_offset = target_offset;
            // Recursively decode the value at the target offset
            decode_value(data, &mut new_offset)
        }
        _ => {
            dbg!(kind, offset);
            unimplemented!() // Reserved values
        }
    }
}

pub fn decode_from_buffer(data: &[u8]) -> Value {
    // Read the last byte to find where the root value starts
    let n = data[data.len() - 1] as usize;
    let mut root_offset = data.len() - n - 2;
    dbg!(root_offset);
    decode_value(data, &mut root_offset)
}



