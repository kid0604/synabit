//! Moving what people already have into the shape that survives syncing.
//!
//! Two things used to live inside a person's frontmatter that should never
//! have been there:
//!
//! * **`interactions`** — a list of objects that grew with every recorded
//!   coffee. A `.md` file is merged character by character when two devices
//!   have both changed it, which is right for prose and wrong for YAML: an
//!   interleave of two versions of the same list is neither of them, and may
//!   not parse. This was the largest such list in the app.
//! * **`relations`** — the same person-to-person links kept a second time as
//!   markdown mentions, so that the edge index would notice them. It reads
//!   the links directly now, and the duplicate had already drifted.
//!
//! # What this does not do
//!
//! It does not delete anything it has not already written somewhere else. An
//! interaction becomes a file first; only then is it taken out of the person.
//! A person whose interactions could not be written keeps them, and the next
//! run tries again.

use serde_json::{json, Map, Value};

/// One file to write, as the caller's node writer wants it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedWrite {
    pub rel_path: String,
    pub title: String,
    pub node_type: &'static str,
    pub properties: Value,
    pub content: String,
}

/// What has to happen to move one person into the new shape.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersonPlan {
    /// One file per interaction. Written before the person is changed.
    pub interactions: Vec<PlannedWrite>,
    /// The patch that takes the old copies out of the person's own file.
    ///
    /// Empty when there is nothing to take out, and an empty patch means the
    /// person's file is not touched at all.
    pub patch: Map<String, Value>,
}

impl PersonPlan {
    pub fn is_empty(&self) -> bool {
        self.interactions.is_empty() && self.patch.is_empty()
    }
}

/// What to call a recorded interaction, in a list of them.
fn title_for(kind: &str, person: &str) -> String {
    let label = match kind {
        "meeting" => "Meeting",
        "call" => "Call",
        "message" => "Message",
        "coffee" => "Coffee",
        "gift" => "Gift",
        _ => "Note",
    };
    format!("{} · {}", label, person)
}

/// Work out what moving this person would take. Reads nothing, writes nothing.
///
/// `new_id` supplies a fresh file name per interaction — passed in rather than
/// generated here so a test can predict the result.
pub fn plan_person(
    person_id: &str,
    person_title: &str,
    person_identity: &str,
    properties: &Value,
    mut new_id: impl FnMut() -> String,
) -> PersonPlan {
    let mut plan = PersonPlan::default();

    for interaction in properties
        .get("interactions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let kind = interaction
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("other");
        let date = interaction
            .get("date")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let note = interaction
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let mood = interaction
            .get("mood")
            .and_then(Value::as_str)
            .filter(|m| !m.trim().is_empty());

        // An entry with neither a date nor a note records nothing. Writing a
        // file for it would turn a stray line of YAML into a permanent row in
        // somebody's timeline.
        if date.is_empty() && note.is_empty() {
            continue;
        }

        let mut props = Map::new();
        // The person's identity, so the link survives their file moving.
        props.insert("person_id".into(), json!(person_identity));
        props.insert("interaction_type".into(), json!(kind));
        props.insert("date".into(), json!(date));
        if let Some(mood) = mood {
            props.insert("mood".into(), json!(mood));
        }

        plan.interactions.push(PlannedWrite {
            rel_path: format!("People/Interactions/{}.md", new_id()),
            title: title_for(kind, person_title),
            node_type: "interaction",
            properties: Value::Object(props),
            content: note,
        });
    }

    // `null` is how a write says "remove this key".
    if properties.get("interactions").is_some() && !plan.interactions.is_empty() {
        plan.patch.insert("interactions".into(), Value::Null);
    }
    // An `interactions` list that produced no files held nothing worth
    // keeping, so it goes too — but only if it was empty to begin with, never
    // because writing failed.
    if let Some(existing) = properties.get("interactions").and_then(Value::as_array) {
        if existing.is_empty() {
            plan.patch.insert("interactions".into(), Value::Null);
        }
    }

    if properties.get("relations").is_some() {
        plan.patch.insert("relations".into(), Value::Null);
    }

    // The name each connection was written with. The graph reads the other
    // person's own row now, so this copy only ever went stale.
    if let Some(connections) = properties.get("connections").and_then(Value::as_array) {
        if connections.iter().any(|c| c.get("name").is_some()) {
            let cleaned: Vec<Value> = connections
                .iter()
                .map(|c| {
                    let mut c = c.as_object().cloned().unwrap_or_default();
                    c.remove("name");
                    Value::Object(c)
                })
                .collect();
            plan.patch.insert("connections".into(), json!(cleaned));
        }
    }

    let _ = person_id;
    plan
}

#[cfg(test)]
mod tests;
