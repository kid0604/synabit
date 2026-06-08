use serde::{Deserialize, Serialize};

/// A feed extracted from an OPML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFeed {
    pub url: String,
    pub title: String,
    pub category: String,
    pub feed_type: String,
}

/// Feed source for export (matches the FeedSource struct shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSource {
    pub url: String,
    pub site_url: String,
    pub title: String,
    pub description: String,
    pub category_name: String,
}

/// Parse OPML content and extract feed entries.
pub fn import_opml(content: &str) -> Result<Vec<ImportedFeed>, String> {
    let document = opml::OPML::from_reader(&mut content.as_bytes())
        .map_err(|e| format!("OPML parse error: {}", e))?;

    let mut feeds = Vec::new();
    collect_outlines(&document.body.outlines, "", &mut feeds);
    Ok(feeds)
}

/// Recursively collect feeds from OPML outlines.
fn collect_outlines(outlines: &[opml::Outline], parent_category: &str, feeds: &mut Vec<ImportedFeed>) {
    for outline in outlines {
        if let Some(ref xml_url) = outline.xml_url {
            // This is a feed entry
            if !xml_url.is_empty() {
                let title = outline
                    .title
                    .clone()
                    .unwrap_or_else(|| outline.text.clone());

                let feed_type = outline
                    .r#type
                    .clone()
                    .unwrap_or_else(|| "rss".to_string());

                feeds.push(ImportedFeed {
                    url: xml_url.clone(),
                    title,
                    category: parent_category.to_string(),
                    feed_type,
                });
            }
        } else if !outline.outlines.is_empty() {
            // This is a category folder
            let category = outline
                .title
                .clone()
                .unwrap_or_else(|| outline.text.clone());
            collect_outlines(&outline.outlines, &category, feeds);
        }
    }
}

/// Export feeds and categories to OPML XML format.
pub fn export_opml(sources: &[ExportSource]) -> String {
    let mut opml_doc = opml::OPML::default();
    opml_doc.head = Some(opml::Head {
        title: Some("Synabit Feed Subscriptions".to_string()),
        date_created: Some(chrono::Utc::now().to_rfc2822()),
        ..Default::default()
    });

    // Group sources by category
    let mut categories: std::collections::HashMap<String, Vec<&ExportSource>> =
        std::collections::HashMap::new();
    for source in sources {
        categories
            .entry(source.category_name.clone())
            .or_default()
            .push(source);
    }

    for (category_name, cat_sources) in &categories {
        let mut children = Vec::new();
        for source in cat_sources {
            children.push(opml::Outline {
                text: source.title.clone(),
                title: Some(source.title.clone()),
                xml_url: Some(source.url.clone()),
                html_url: Some(source.site_url.clone()),
                r#type: Some("rss".to_string()),
                description: Some(source.description.clone()),
                ..Default::default()
            });
        }

        if category_name.is_empty() {
            // Uncategorized feeds go at root level
            opml_doc.body.outlines.extend(children);
        } else {
            opml_doc.body.outlines.push(opml::Outline {
                text: category_name.clone(),
                title: Some(category_name.clone()),
                outlines: children,
                ..Default::default()
            });
        }
    }

    opml_doc.to_string().unwrap_or_else(|_| String::new())
}
