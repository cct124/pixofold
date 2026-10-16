//! UTF-8 XMP的有界XML/命名空间检查；保持原包字节，不执行DTD或下载外部资源。

use super::{invalid, protected};
use crate::jpeg::JpegError;
use quick_xml::{
    events::Event,
    name::{Namespace, ResolveResult},
    reader::NsReader,
};

pub(super) const PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

fn namespace(ns: ResolveResult<'_>) -> Result<&str, JpegError> {
    match ns {
        ResolveResult::Bound(Namespace(value)) => Ok(value),
        ResolveResult::Unbound => Ok(""),
        ResolveResult::Unknown(_) => Err(invalid("XMP未绑定命名空间")),
    }
}

fn property(ns: &str, name: &str) -> Result<(), JpegError> {
    // 外链凭据和扩展XMP须独立处理，不能当普通描述保留或随APP11一起删除。
    if (ns == "http://purl.org/dc/terms/" && name == "provenance")
        || ns == "http://www.w3.org/2000/09/xmldsig#"
        || (ns == "http://ns.adobe.com/xmp/note/" && name == "HasExtendedXMP")
    {
        return Err(protected(0xe1));
    }
    Ok(())
}

pub(super) fn inspect(bytes: &[u8]) -> Result<(), JpegError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("XMP不是UTF-8"))?;
    if text.chars().any(|c| {
        (c < ' ' && !matches!(c, '\t' | '\r' | '\n')) || matches!(c, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(invalid("XMP包含非法字符"));
    }
    let mut reader = NsReader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut depth = 0usize;
    let mut elements = 0usize;
    let mut roots = 0;
    let mut declaration_seen = false;
    let mut rdf = false;
    loop {
        let event = reader.read_event().map_err(|_| invalid("XMP XML无效"))?;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                elements += 1;
                if elements > 16384 || depth >= 64 {
                    return Err(JpegError::ResourceLimit("XMP结构"));
                }
                if depth == 0 {
                    roots += 1;
                }
                let (ns, local) = reader.resolver().resolve_element(element.name());
                let ns = namespace(ns)?;
                property(ns, local.as_ref())?;
                if ns == "http://www.w3.org/1999/02/22-rdf-syntax-ns#" && local.as_ref() == "RDF" {
                    rdf = true;
                }
                let mut expanded = std::collections::HashSet::new();
                for (i, attribute) in element.attributes().enumerate() {
                    if i >= 128 {
                        return Err(JpegError::ResourceLimit("XMP属性数"));
                    }
                    let attribute = attribute.map_err(|_| invalid("XMP属性无效"))?;
                    let value = attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| invalid("XMP属性实体无效"))?;
                    if value.chars().any(|c| {
                        (c < ' ' && !matches!(c, '\t' | '\r' | '\n'))
                            || matches!(c, '\u{fffe}' | '\u{ffff}')
                    }) {
                        return Err(invalid("XMP属性字符无效"));
                    }
                    if attribute.key.as_ref() == "xmlns"
                        || attribute.key.as_ref().starts_with("xmlns:")
                    {
                        // 命名空间URI不能借实体与解析器原始绑定产生不同解释。
                        if attribute.value.as_ref() != value.as_ref() {
                            return Err(invalid("XMP命名空间转义"));
                        }
                        continue;
                    }
                    let (ns, local) = reader.resolver().resolve_attribute(attribute.key);
                    let ns = namespace(ns)?;
                    property(ns, local.as_ref())?;
                    if !expanded.insert((ns, local.as_ref().to_owned())) {
                        return Err(invalid("XMP扩展属性名重复"));
                    }
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(|| invalid("XMP层级无效"))?;
            }
            Event::Text(text)
                if depth == 0 && !text.as_ref().bytes().all(|b| b.is_ascii_whitespace()) =>
            {
                return Err(invalid("XMP根外文本"));
            }
            Event::CData(_) if depth == 0 => return Err(invalid("XMP根外文本")),
            Event::GeneralRef(entity) => {
                let name = entity.as_ref();
                let encoded = format!("&{name};");
                let decoded =
                    quick_xml::escape::unescape(&encoded).map_err(|_| invalid("XMP实体无效"))?;
                if depth == 0
                    || decoded.chars().any(|c| {
                        (c < ' ' && !matches!(c, '\t' | '\r' | '\n'))
                            || matches!(c, '\u{fffe}' | '\u{ffff}')
                    })
                {
                    return Err(invalid("XMP实体无效"));
                }
            }
            Event::DocType(_) => return Err(protected(0xe1)),
            Event::Decl(declaration) => {
                if declaration_seen
                    || roots != 0
                    || !declaration.version().is_ok_and(|v| v.as_ref() == "1.0")
                    || declaration
                        .encoding()
                        .is_some_and(|value| !value.is_ok_and(|v| v.eq_ignore_ascii_case("UTF-8")))
                {
                    return Err(invalid("XMP编码声明无效"));
                }
                declaration_seen = true;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if roots != 1 || depth != 0 || !rdf {
        return Err(invalid("XMP根或RDF无效"));
    }
    Ok(())
}
