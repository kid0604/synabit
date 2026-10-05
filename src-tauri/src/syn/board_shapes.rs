//! The shapes a board can draw, as Syn knows them.
//!
//! The list is the app's own — `shapes.ts`, read at compile time — so Syn
//! knows every shape the board has, not the handful written into a prompt,
//! and a shape added to the board is one Syn can use without anyone
//! remembering to tell it. A name the board does not have is mapped from the
//! words a model reaches for ("database", "server", "person"), and failing
//! that is a plain box: a shape stored under a name the board does not know is
//! drawn as a rectangle anyway, and then can never be found again by name.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../../../src/mini-apps/whiteboard/shapes.ts");

struct Catalog {
    /// Lower-cased id → the id as the board writes it.
    ids: HashMap<String, &'static str>,
    /// Shapes that are pictures of a thing, drawn square with their words under them.
    figures: HashSet<&'static str>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut ids = HashMap::new();
        let mut figures = HashSet::new();
        // Each entry starts `{ id: 'name'`; what follows up to the next entry
        // is that shape's definition.
        let mut rest = SOURCE;
        while let Some(at) = rest.find("{ id: '") {
            let after = &rest[at + 7..];
            let Some(end) = after.find('\'') else { break };
            let id = &after[..end];
            let body_end = after[end..].find("{ id: '").map(|i| i + end).unwrap_or(after.len());
            if after[end..body_end].contains("labelBelow: true") {
                figures.insert(id);
            }
            ids.insert(id.to_lowercase(), id);
            rest = &after[end..];
        }
        Catalog { ids, figures }
    })
}

/// The words a model uses for a shape, and the board's name for it.
const ALIASES: &[(&str, &str)] = &[
    ("box", "rectangle"), ("rounded", "roundedRect"), ("circle", "ellipse"), ("oval", "ellipse"),
    ("decision", "diamond"), ("database", "cylinder"), ("db", "cylinder"), ("store", "cylinder"),
    ("doc", "document"), ("data", "dataIO"), ("io", "dataIO"), ("person", "umlActor"), ("user", "umlActor"),
    ("actor", "umlActor"), ("server", "netServer"), ("desktop", "netDesktop"), ("laptop", "netLaptop"),
    ("mobile", "netMobile"), ("phone", "netMobile"), ("router", "netRouter"), ("switch", "netSwitch"),
    ("firewall", "netFirewall"), ("internet", "netGlobe"), ("globe", "netGlobe"), ("queue", "netQueue"),
    ("loadbalancer", "netLoadBalancer"), ("printer", "netPrinter"), ("storage", "netStorageArray"),
    ("container", "netContainer"), ("users", "netUsers"), ("building", "netBuilding"), ("terminal", "netTerminal"),
    ("start", "bpmnEvent"), ("end", "bpmnEnd"), ("gateway", "bpmnGateway"), ("task", "bpmnTask"),
    ("note", "note"), ("start/end", "pill"), ("terminator", "pill"),
];

/// The board's name for a shape someone asked for, or None for a plain box.
pub fn board_shape(asked: &str) -> Option<&'static str> {
    let key = asked.trim().to_lowercase().replace([' ', '_', '-'], "");
    let cat = catalog();
    cat.ids
        .get(&key)
        .copied()
        .or_else(|| ALIASES.iter().find(|(word, _)| word.replace([' ', '_', '-', '/'], "") == key.replace('/', "")).map(|(_, id)| *id))
        .filter(|id| cat.ids.contains_key(&id.to_lowercase()))
}

/// Whether a shape is a picture of a thing (a person, a server, a phone)
/// rather than a box to write in.
pub fn is_figure(id: &str) -> bool {
    catalog().figures.contains(id)
}

/// How many shapes the board has: for the tool's description.
pub fn count() -> usize {
    catalog().ids.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_of_the_board_is_known_and_the_words_for_them_too() {
        assert!(count() > 300, "{}", count());
        assert_eq!(board_shape("cylinder"), Some("cylinder"));
        assert_eq!(board_shape("bpmnTaskUser"), Some("bpmnTaskUser"));
        assert_eq!(board_shape("BPMN task user"), Some("bpmnTaskUser"));
        assert_eq!(board_shape("database"), Some("cylinder"));
        assert_eq!(board_shape("load balancer"), Some("netLoadBalancer"));
        assert_eq!(board_shape("nonsense"), None);
        assert_eq!(board_shape("constructor"), None);
    }

    #[test]
    fn figures_are_the_shapes_drawn_with_their_words_under_them() {
        assert!(is_figure("umlActor") && is_figure("netServer") && is_figure("infoLightbulb"));
        assert!(!is_figure("rectangle") && !is_figure("netCloud"));
    }
}
