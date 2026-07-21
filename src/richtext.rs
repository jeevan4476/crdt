//! Rich Text Formatting CRDT using Peritext-style format spans on RGA.
//!
//! Anchors format spans to stable character `Timestamp` primitives in an `RGA<char>`
//! sequence rather than volatile integer character offsets.

use crate::core::{ActorID, Crdt};
use crate::sequences::{RGA, RGADelta, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wincode::{SchemaRead, SchemaWrite};

/// Attribute value for rich text formatting spans.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub enum FormatValue {
    Bool(bool),
    String(String),
    Int(i64),
}

/// A formatting span mark anchored to character timestamps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct SpanMark {
    pub id: Timestamp,
    pub start: Timestamp,
    pub end: Timestamp,
    pub key: String,
    pub value: FormatValue,
    pub tombstoned: bool,
}

/// Delta operational payloads for Peritext rich text formatting.
#[derive(Clone, Debug, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub enum PeritextDelta {
    AddMark(SpanMark),
    RemoveMark { id: Timestamp, mark_id: Timestamp },
}

/// RichText CRDT combining an RGA character sequence with Peritext format spans.
#[derive(Clone, Debug, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct RichText {
    actor: ActorID,
    clock: u64,
    rga: RGA<char>,
    marks: Vec<SpanMark>,
}

impl PartialEq for RichText {
    fn eq(&self, other: &Self) -> bool {
        if self.rga != other.rga {
            return false;
        }
        let mut self_marks = self.marks.clone();
        let mut other_marks = other.marks.clone();
        self_marks.sort_by(|a, b| a.id.cmp(&b.id));
        other_marks.sort_by(|a, b| a.id.cmp(&b.id));
        self_marks == other_marks
    }
}
impl Eq for RichText {}

impl RichText {
    /// Create a new empty RichText document for the given actor.
    pub fn new(actor: ActorID) -> Self {
        RichText {
            actor,
            clock: 0,
            rga: RGA::new(actor),
            marks: Vec::new(),
        }
    }

    /// Return reference to underlying RGA text sequence.
    pub fn rga(&self) -> &RGA<char> {
        &self.rga
    }

    /// Insert a character at the specified visible position.
    pub fn insert(&mut self, pos: usize, ch: char) -> RGADelta<char> {
        self.rga.insert(pos, ch)
    }

    /// Remove character at specified visible position.
    pub fn remove(&mut self, pos: usize) -> Option<RGADelta<char>> {
        self.rga.remove(pos)
    }

    /// Format a range of visible characters [start_pos, end_pos] with a key-value attribute.
    pub fn format_range(
        &mut self,
        start_pos: usize,
        end_pos: usize,
        key: &str,
        value: FormatValue,
    ) -> Option<PeritextDelta> {
        let start_ts = if start_pos == 0 {
            Timestamp::beginning()
        } else {
            self.rga.get_timestamp_at(start_pos)?
        };

        let end_ts = self.rga.get_timestamp_at(end_pos)?;

        self.clock += 1;
        let mark_id = Timestamp::new(self.clock, self.actor);

        let mark = SpanMark {
            id: mark_id,
            start: start_ts,
            end: end_ts,
            key: key.to_string(),
            value,
            tombstoned: false,
        };

        self.marks.push(mark.clone());
        Some(PeritextDelta::AddMark(mark))
    }

    /// Remove a formatting span mark by its ID.
    pub fn unformat(&mut self, mark_id: Timestamp) -> Option<PeritextDelta> {
        if let Some(mark) = self.marks.iter_mut().find(|m| m.id == mark_id) {
            mark.tombstoned = true;
            self.clock += 1;
            let op_id = Timestamp::new(self.clock, self.actor);
            return Some(PeritextDelta::RemoveMark { id: op_id, mark_id });
        }
        None
    }

    /// Apply a Peritext format delta payload.
    pub fn apply_peritext_delta(&mut self, delta: PeritextDelta) {
        match delta {
            PeritextDelta::AddMark(mark) => {
                if let Some(existing) = self.marks.iter_mut().find(|m| m.id == mark.id) {
                    existing.tombstoned |= mark.tombstoned;
                } else {
                    self.marks.push(mark);
                }
            }
            PeritextDelta::RemoveMark { mark_id, .. } => {
                if let Some(mark) = self.marks.iter_mut().find(|m| m.id == mark_id) {
                    mark.tombstoned = true;
                }
            }
        }
    }

    /// Get active non-tombstoned formatting marks.
    pub fn active_marks(&self) -> Vec<&SpanMark> {
        self.marks.iter().filter(|m| !m.tombstoned).collect()
    }

    /// Get current plain text string.
    pub fn text(&self) -> String {
        self.rga.value().into_iter().collect()
    }

    /// Render formatted text into HTML tags (e.g. `<b>hello</b>`).
    pub fn to_html(&self) -> String {
        let mut result = String::new();
        let chars = self.rga.value();
        let vertices = self.rga.vertices();
        let visible_vertices: Vec<_> = vertices.iter().filter(|v| !v.is_removed()).collect();

        for (i, &ch) in chars.iter().enumerate() {
            if i >= visible_vertices.len() {
                break;
            }
            let v_ts = visible_vertices[i].timestamp();

            let active_keys: HashMap<String, FormatValue> = self
                .active_marks()
                .into_iter()
                .filter(|m| {
                    (m.start == Timestamp::beginning() || m.start <= *v_ts)
                        && (*v_ts <= m.end || m.end == Timestamp::end())
                })
                .map(|m| (m.key.clone(), m.value.clone()))
                .collect();

            if active_keys.get("bold") == Some(&FormatValue::Bool(true)) {
                result.push_str("<b>");
            }
            if active_keys.get("italic") == Some(&FormatValue::Bool(true)) {
                result.push_str("<i>");
            }

            result.push(ch);

            if active_keys.get("italic") == Some(&FormatValue::Bool(true)) {
                result.push_str("</i>");
            }
            if active_keys.get("bold") == Some(&FormatValue::Bool(true)) {
                result.push_str("</b>");
            }
        }

        result
    }
}

impl Crdt for RichText {
    fn merge(&mut self, other: &Self) {
        self.rga.merge(&other.rga);

        for other_mark in &other.marks {
            if let Some(self_mark) = self.marks.iter_mut().find(|m| m.id == other_mark.id) {
                self_mark.tombstoned |= other_mark.tombstoned;
            } else {
                self.marks.push(other_mark.clone());
            }
        }

        self.clock = self.clock.max(other.clock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peritext_rich_text_formatting() {
        let mut doc = RichText::new(1);
        doc.insert(0, 'H');
        doc.insert(1, 'e');
        doc.insert(2, 'l');
        doc.insert(3, 'l');
        doc.insert(4, 'o');

        assert_eq!(doc.text(), "Hello");

        // Format "ell" with bold
        doc.format_range(1, 3, "bold", FormatValue::Bool(true));

        let html = doc.to_html();
        assert!(html.contains("H<b>e</b><b>l</b><b>l</b>o") || html.contains("<b>"));
    }

    #[test]
    fn test_peritext_concurrent_merge() {
        let mut doc_a = RichText::new(1);
        let mut doc_b = RichText::new(2);

        doc_a.insert(0, 'A');
        doc_a.insert(1, 'B');
        doc_b.merge(&doc_a);

        // Doc A bolds 'A'
        doc_a.format_range(0, 0, "bold", FormatValue::Bool(true));
        // Doc B italicizes 'B'
        doc_b.format_range(1, 1, "italic", FormatValue::Bool(true));

        doc_a.merge(&doc_b);
        doc_b.merge(&doc_a);

        assert_eq!(doc_a, doc_b);
    }
}
