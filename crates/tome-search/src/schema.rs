//! Tantivy schema. Single source of truth for the field definitions used by
//! both the writer and the searcher.

use tantivy::schema::{
    Field, INDEXED, STORED, STRING, Schema, SchemaBuilder, TEXT, TextFieldIndexing, TextOptions,
};

#[derive(Clone)]
pub struct TomeSchema {
    pub schema: Schema,
    pub page_id: Field,
    pub title: Field,
    pub body: Field,
    pub tier: Field,
}

impl TomeSchema {
    pub fn build() -> Self {
        let mut builder = SchemaBuilder::new();

        // page_id: unique numeric key, stored so we can return it in hits.
        let page_id = builder.add_u64_field("page_id", INDEXED | STORED);

        // title: stored for display and tokenized for matching.
        let title_options = TextOptions::default().set_stored().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions),
        );
        let title = builder.add_text_field("title", title_options);

        // body: tokenized; not stored to keep index size down. Snippets can
        // be regenerated from the dump later if needed.
        let body = builder.add_text_field("body", TEXT);

        // tier: short string for filtering. STRING = exact-match indexed.
        let tier = builder.add_text_field("tier", STRING | STORED);

        let schema = builder.build();

        Self {
            schema,
            page_id,
            title,
            body,
            tier,
        }
    }
}
