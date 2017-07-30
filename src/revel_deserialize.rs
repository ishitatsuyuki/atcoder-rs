use serde::de;
use std::fmt::Display;
use percent_encoding::percent_decode;

error_chain! {
    errors {
        Message(msg: String)
        InvalidInput {
            description("Invalid input")
        }
        Eof {
            description("Unexpected end-of-stream")
        }
    }
}

impl de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        ErrorKind::Message(msg.to_string()).into()
    }
}

pub struct Deserializer<'de> {
    input: ::std::iter::Peekable<
        ::std::iter::Map<::percent_encoding::PercentDecode<'de>, fn(u8) -> u8>,
    >,
}

impl<'de> Deserializer<'de> {
    /// Create a revel deserializer.
    pub fn from_bytes(input: &'de [u8]) -> Self {
        fn transform(x: u8) -> u8 {
            if x == b'+' {
                b' '
            } else {
                x
            }
        }
        Deserializer {
            input: percent_decode(input)
                .map(transform as fn(u8) -> u8)
                .peekable(),
        }
    }

    fn next_byte(&mut self) -> Result<u8> {
        self.input.next().ok_or(ErrorKind::Eof.into())
    }

    fn parse_sequence(&mut self) -> Result<Vec<u8>> {
        match self.input.clone().position(|x| x == b':' || x == 0) {
            Some(len) => Ok((&mut self.input).take(len).collect()),
            None => {
                bail!(ErrorKind::Eof);
            }
        }
    }
}

impl<'a, 'de> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_string(String::from_utf8(self.parse_sequence()?)
            .chain_err(|| ErrorKind::InvalidInput)?)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_map(SequenceParser { de: self })
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_i64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_u32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_u64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }
}

struct SequenceParser<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
}

impl<'de, 'a> de::MapAccess<'de> for SequenceParser<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        if self.de.input.peek().is_none() {
            Ok(None)
        } else {
            ensure!(self.de.next_byte()? == 0, ErrorKind::InvalidInput);
            seed.deserialize(&mut *self.de).map(Some)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        ensure!(self.de.next_byte()? == b':', ErrorKind::InvalidInput);
        let result = seed.deserialize(&mut *self.de);
        ensure!(self.de.next_byte()? == 0, ErrorKind::InvalidInput);
        result
    }
}

pub fn from_bytes<'a, T>(input: &'a [u8]) -> Result<T>
where
    T: de::Deserialize<'a>,
{
    let mut de = Deserializer::from_bytes(input);
    de::Deserialize::deserialize(&mut de)
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct RevelFlash {
    pub success: Option<String>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse() {
        use std::collections::HashMap;
        let input = b"%00a%3Ab%00%00c%3Ad%00";
        let expected: HashMap<_, _> = [("a", "b"), ("c", "d")]
            .iter()
            .map(|&(x, y)| (x.to_owned(), y.to_owned()))
            .collect();
        let actual = super::from_bytes(&*input).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_flash_error() {
        let input = b"%00error%3AWrong+password%00%00unneeded%3Aa%00";
        let expected = super::RevelFlash {
            success: None,
            error: Some("Wrong password".to_owned()),
        };
        let actual = super::from_bytes(&*input).unwrap();
        assert_eq!(expected, actual);
    }
}
