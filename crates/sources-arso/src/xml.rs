//! A small reader for the XML that ARSO writes: one root element, elements
//! with attributes, text, empty elements, and nothing else that matters.
//!
//! No crate of the workspace reads XML, and the file is simple and flat, so
//! the reader is written here. It reads a document into a tree and refuses
//! what it does not understand rather than guess: a document type
//! declaration, an unknown entity, an end tag that does not match, an
//! element left open, anything after the root.
//!
//! What it understands: the XML declaration and processing instructions
//! (skipped), comments (skipped), CDATA sections (text), attributes in
//! single or double quotes, empty elements (`<x/>`), the five predefined
//! entities and character references (`&#38;`, `&#x26;`).

use std::fmt;

/// One element: its name, its attributes, its text and its children.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    /// The element's name, with its prefix if it has one.
    pub name: String,
    /// The attributes, in the order written.
    pub attributes: Vec<(String, String)>,
    /// The element's own text, every piece of it joined, entities decoded.
    pub text: String,
    /// The child elements, in order.
    pub children: Vec<Element>,
}

/// What the tests look up; the adapter walks the children itself.
#[cfg(test)]
impl Element {
    /// The first child of that name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The value of an attribute.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Why a document could not be read, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError {
    /// The byte offset in the document at which reading stopped.
    pub at: usize,
    /// What was wrong.
    pub reason: String,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.reason, self.at)
    }
}

impl std::error::Error for XmlError {}

/// Reads a document and returns its root element.
///
/// # Errors
///
/// If the text is not a document this reader understands.
pub fn parse(text: &str) -> Result<Element, XmlError> {
    let mut reader = Reader { text, pos: 0 };
    // A byte order mark is allowed before everything.
    if reader.rest().starts_with('\u{feff}') {
        reader.pos += '\u{feff}'.len_utf8();
    }
    reader.skip_misc()?;
    if !reader.rest().starts_with('<') {
        return Err(reader.error("no root element"));
    }
    let root = reader.element()?;
    reader.skip_misc()?;
    if !reader.rest().is_empty() {
        return Err(reader.error("content after the root element"));
    }
    Ok(root)
}

struct Reader<'a> {
    text: &'a str,
    pos: usize,
}

/// Elements may nest this deep; deeper is refused rather than read with
/// unbounded recursion.
const MAX_DEPTH: usize = 64;

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn error(&self, reason: impl Into<String>) -> XmlError {
        XmlError {
            at: self.pos,
            reason: reason.into(),
        }
    }

    /// Skips white space, comments and processing instructions, which may
    /// stand before and after the root element.
    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            let trimmed = self.rest().trim_start();
            self.pos = self.text.len() - trimmed.len();
            if trimmed.starts_with("<?") {
                self.skip_past("?>", "a processing instruction is not closed")?;
            } else if trimmed.starts_with("<!--") {
                self.skip_past("-->", "a comment is not closed")?;
            } else if trimmed.starts_with("<!") {
                return Err(self.error("a document type declaration is not read"));
            } else {
                return Ok(());
            }
        }
    }

    fn skip_past(&mut self, end: &str, reason: &str) -> Result<(), XmlError> {
        match self.rest().find(end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(())
            }
            None => Err(self.error(reason)),
        }
    }

    fn element(&mut self) -> Result<Element, XmlError> {
        self.element_at(0)
    }

    /// Reads an element that starts at the current position with `<`.
    fn element_at(&mut self, depth: usize) -> Result<Element, XmlError> {
        if depth > MAX_DEPTH {
            return Err(self.error("elements nest too deep"));
        }
        self.pos += 1; // '<'
        let name = self.name()?;
        let mut element = Element {
            name,
            ..Element::default()
        };
        // Attributes, up to '>' or '/>'.
        loop {
            self.skip_space();
            let rest = self.rest();
            if let Some(after) = rest.strip_prefix("/>") {
                self.pos = self.text.len() - after.len();
                return Ok(element);
            }
            if rest.starts_with('>') {
                self.pos += 1;
                break;
            }
            if rest.is_empty() {
                return Err(self.error(format!("the tag <{}> is not closed", element.name)));
            }
            let at = self.pos;
            let name = self.name()?;
            if !self.rest().chars().next().is_some_and(char::is_whitespace)
                && !self.rest().starts_with('=')
            {
                return Err(self.error(format!("attribute {name} has no value")));
            }
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err(self.error(format!("attribute {name} has no value")));
            }
            self.pos += 1;
            self.skip_space();
            let value = self.quoted()?;
            if element.attributes.iter().any(|(n, _)| *n == name) {
                return Err(XmlError {
                    at,
                    reason: format!("attribute {name} is written twice"),
                });
            }
            element.attributes.push((name, value));
        }
        // Content, up to the matching end tag.
        loop {
            let rest = self.rest();
            if rest.is_empty() {
                return Err(self.error(format!("the element <{}> is not closed", element.name)));
            }
            if let Some(after) = rest.strip_prefix("</") {
                self.pos = self.text.len() - after.len();
                let name = self.name()?;
                self.skip_space();
                if !self.rest().starts_with('>') {
                    return Err(self.error(format!("the end tag </{name}> is not closed")));
                }
                self.pos += 1;
                if name != element.name {
                    return Err(self.error(format!(
                        "the end tag </{name}> does not close <{}>",
                        element.name
                    )));
                }
                return Ok(element);
            }
            if rest.starts_with("<!--") {
                self.skip_past("-->", "a comment is not closed")?;
            } else if let Some(after) = rest.strip_prefix("<![CDATA[") {
                let Some(end) = after.find("]]>") else {
                    return Err(self.error("a CDATA section is not closed"));
                };
                element.text.push_str(&after[..end]);
                self.pos = self.text.len() - after.len() + end + 3;
            } else if rest.starts_with("<?") {
                self.skip_past("?>", "a processing instruction is not closed")?;
            } else if rest.starts_with("<!") {
                return Err(self.error("a declaration inside an element is not read"));
            } else if rest.starts_with('<') {
                let child = self.element_at(depth + 1)?;
                element.children.push(child);
            } else {
                let end = rest.find('<').unwrap_or(rest.len());
                let raw = &rest[..end];
                let text = self.decode(raw)?;
                element.text.push_str(&text);
                self.pos += end;
            }
        }
    }

    fn skip_space(&mut self) {
        let trimmed = self.rest().trim_start();
        self.pos = self.text.len() - trimmed.len();
    }

    /// An element's or an attribute's name.
    fn name(&mut self) -> Result<String, XmlError> {
        let rest = self.rest();
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '>' | '/' | '=' | '<' | '"' | '\''))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let valid_start = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == ':');
        if !valid_start {
            return Err(self.error("a name is missing or does not start with a letter"));
        }
        self.pos += end;
        Ok(name.to_owned())
    }

    /// An attribute's value in single or double quotes, decoded.
    fn quoted(&mut self) -> Result<String, XmlError> {
        let rest = self.rest();
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            return Err(self.error("an attribute's value is not in quotes"));
        };
        let Some(end) = rest[1..].find(quote) else {
            return Err(self.error("an attribute's value is not closed"));
        };
        let raw = &rest[1..=end];
        if raw.contains('<') {
            return Err(self.error("an attribute's value holds '<'"));
        }
        let value = self.decode(raw)?;
        self.pos += end + 2;
        Ok(value)
    }

    /// Decodes the entities and character references of a piece of text.
    fn decode(&self, raw: &str) -> Result<String, XmlError> {
        if !raw.contains('&') {
            return Ok(raw.to_owned());
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(i) = rest.find('&') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let Some(end) = after.find(';') else {
                return Err(self.error("an '&' that starts no entity"));
            };
            let entity = &after[..end];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => {
                    let number = if let Some(hex) = entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                    {
                        u32::from_str_radix(hex, 16).ok()
                    } else if let Some(decimal) = entity.strip_prefix('#') {
                        decimal.parse::<u32>().ok()
                    } else {
                        None
                    };
                    number.and_then(char::from_u32)
                }
            };
            let Some(c) = decoded else {
                return Err(self.error(format!("the entity &{entity}; is not known")));
            };
            out.push(c);
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, text: &str) -> Element {
        Element {
            name: name.to_owned(),
            text: text.to_owned(),
            ..Element::default()
        }
    }

    #[test]
    fn reads_a_flat_document_in_arsos_manner() {
        let doc = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><data id=\"MeteoSI\">\
                   <credit>Agencija</credit><metData><t>-0.5</t><tx/><rh>99</rh>\
                   <webcam view-direction=\"se\">K_dir/a.jpg</webcam></metData></data>";
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "data");
        assert_eq!(root.attribute("id"), Some("MeteoSI"));
        assert_eq!(root.child("credit").unwrap().text, "Agencija");
        let met = root.child("metData").unwrap();
        assert_eq!(met.children.len(), 4);
        assert_eq!(met.children[0], leaf("t", "-0.5"));
        assert_eq!(met.children[1], leaf("tx", ""));
        assert_eq!(met.children[2], leaf("rh", "99"));
        assert_eq!(met.children[3].attribute("view-direction"), Some("se"));
        assert_eq!(met.children[3].text, "K_dir/a.jpg");
    }

    #[test]
    fn keeps_text_as_written_with_its_letters() {
        let root = parse("<a><n>Kočevje</n><m>Tržaški zaliv (Zarja)</m><u>°C</u></a>").unwrap();
        assert_eq!(root.children[0].text, "Kočevje");
        assert_eq!(root.children[1].text, "Tržaški zaliv (Zarja)");
        assert_eq!(root.children[2].text, "°C");
    }

    #[test]
    fn decodes_entities_and_character_references() {
        let root =
            parse("<a x='&quot;1&apos;'>A &amp; B &lt;C&gt; &#269;&#x161;&#X17E;</a>").unwrap();
        assert_eq!(root.text, "A & B <C> čšž");
        assert_eq!(root.attribute("x"), Some("\"1'"));
    }

    #[test]
    fn reads_cdata_comments_and_white_space() {
        let doc = "\u{feff}<?xml version=\"1.0\"?>\n<!-- head -->\n<a>\n  <b><![CDATA[x < y & z]]></b>\n  <!-- inner -->\n  <c >1</c >\n</a>\n<!-- tail -->\n";
        let root = parse(doc).unwrap();
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].text, "x < y & z");
        assert_eq!(root.children[1].text, "1");
    }

    #[test]
    fn refuses_what_is_not_a_document() {
        for (doc, reason) in [
            ("", "no root element"),
            ("   ", "no root element"),
            ("plain text", "no root element"),
            ("<a><b></a>", "does not close"),
            ("<a><b>1</b>", "is not closed"),
            ("<a>1</a><b/>", "after the root"),
            ("<a>1</a>trailing", "after the root"),
            ("<a>&nbsp;</a>", "not known"),
            ("<a>A & B</a>", "starts no entity"),
            ("<a>A &B; C</a>", "not known"),
            ("<a>&amp</a>", "starts no entity"),
            ("<a>&#xD800;</a>", "not known"),
            ("<a x=1/>", "not in quotes"),
            ("<a x='1/>", "not closed"),
            ("<a x/>", "no value"),
            ("<a x='1' x='2'/>", "twice"),
            ("<a x='<'/>", "holds '<'"),
            ("<a", "not closed"),
            ("<a><![CDATA[x</a>", "CDATA"),
            ("<a><!-- x</a>", "comment"),
            ("<!DOCTYPE a><a/>", "document type"),
            ("<a><!ENTITY x 'y'></a>", "declaration"),
            ("<1a/>", "name"),
            ("<a></ a>", "name"),
            ("<html>502 Bad Gateway", "is not closed"),
        ] {
            let error = parse(doc).expect_err(doc);
            assert!(
                error.reason.contains(reason),
                "{doc:?}: {error} (wanted {reason:?})"
            );
        }
    }

    #[test]
    fn refuses_elements_nested_without_end() {
        let deep = "<a>".repeat(MAX_DEPTH + 2) + &"</a>".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).unwrap_err().reason.contains("too deep"));
        let fine = "<a>".repeat(MAX_DEPTH) + &"</a>".repeat(MAX_DEPTH);
        assert!(parse(&fine).is_ok());
    }

    #[test]
    fn reads_the_whole_recorded_file() {
        let text = include_str!("../tests/fixtures/observationAms_si_latest.xml");
        let root = parse(text).unwrap();
        assert_eq!(root.name, "data");
        assert_eq!(
            root.attribute("id"),
            Some("MeteoSI_WebMet_observationAms_xml")
        );
        let stations: Vec<&Element> = root
            .children
            .iter()
            .filter(|c| c.name == "metData")
            .collect();
        assert_eq!(stations.len(), 15);
        // Every value of a station is a leaf; only the webcams have attributes.
        for station in &stations {
            for child in &station.children {
                assert!(child.children.is_empty(), "{}", child.name);
                assert!(
                    child.attributes.is_empty() || child.name == "webcam",
                    "{}",
                    child.name
                );
            }
        }
    }
}
