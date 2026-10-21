// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum ChunkCompression {
    Plain,
    Zstd,
}
impl<'a> postgres_types::ToSql for ChunkCompression {
    fn to_sql(
        &self,
        ty: &postgres_types::Type,
        buf: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let s = match *self {
            ChunkCompression::Plain => "plain",
            ChunkCompression::Zstd => "zstd",
        };
        buf.extend_from_slice(s.as_bytes());
        std::result::Result::Ok(postgres_types::IsNull::No)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "chunk_compression" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "plain" => true,
                    "zstd" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
    fn to_sql_checked(
        &self,
        ty: &postgres_types::Type,
        out: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        postgres_types::__to_sql_checked(self, ty, out)
    }
}
impl<'a> postgres_types::FromSql<'a> for ChunkCompression {
    fn from_sql(
        ty: &postgres_types::Type,
        buf: &'a [u8],
    ) -> Result<ChunkCompression, Box<dyn std::error::Error + Sync + Send>> {
        match std::str::from_utf8(buf)? {
            "plain" => Ok(ChunkCompression::Plain),
            "zstd" => Ok(ChunkCompression::Zstd),
            s => Result::Err(Into::into(format!("invalid variant `{}`", s))),
        }
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "chunk_compression" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "plain" => true,
                    "zstd" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
}
