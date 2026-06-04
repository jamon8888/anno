use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const CORPUS_NAMESPACE: Uuid = Uuid::from_u128(0x3a17_0c2f_9db3_4b42_b73f_7be4_f565_4a01);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorpusId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentInstanceId(Uuid);

impl CorpusId {
    #[must_use]
    pub const fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    #[must_use]
    pub fn as_string(self) -> String {
        self.0.to_string()
    }

    #[must_use]
    pub fn from_normalized_root(normalized_root: &str) -> Self {
        Self(stable_uuid(&["corpus", normalized_root]))
    }
}

impl ContentId {
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hex_lower(&hasher.finalize()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl DocumentInstanceId {
    #[must_use]
    pub const fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    #[must_use]
    pub fn as_string(self) -> String {
        self.0.to_string()
    }

    #[must_use]
    pub fn from_parts(
        corpus_id: CorpusId,
        normalized_relative_path: &str,
        content_id: &ContentId,
    ) -> Self {
        Self(stable_uuid(&[
            "document",
            &corpus_id.as_string(),
            normalized_relative_path,
            content_id.as_str(),
        ]))
    }
}

fn stable_uuid(parts: &[&str]) -> Uuid {
    Uuid::new_v5(&CORPUS_NAMESPACE, parts.join("\u{1f}").as_bytes())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_id_is_stable_for_normalized_root() {
        let a = CorpusId::from_normalized_root("c:/clients/acme");
        let b = CorpusId::from_normalized_root("c:/clients/acme");
        assert_eq!(a, b);
    }

    #[test]
    fn same_content_in_two_corpora_keeps_content_id_but_changes_document_instance_id() {
        let content_a = ContentId::from_bytes(b"same file");
        let content_b = ContentId::from_bytes(b"same file");
        let corpus_a = CorpusId::from_normalized_root("c:/clients/a");
        let corpus_b = CorpusId::from_normalized_root("c:/clients/b");
        let doc_a = DocumentInstanceId::from_parts(corpus_a, "contract.pdf", &content_a);
        let doc_b = DocumentInstanceId::from_parts(corpus_b, "contract.pdf", &content_b);

        assert_eq!(content_a, content_b);
        assert_ne!(doc_a, doc_b);
    }
}
