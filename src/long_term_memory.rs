use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use tokio_postgres::{Client, NoTls};

const DEFAULT_CHUNK_CHARS: usize = 1_800;
const EMBEDDING_DIMENSIONS: usize = 384;

#[derive(Debug)]
pub struct LongTermMemory {
    client: Client,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownChunk {
    pub source_path: PathBuf,
    pub chunk_index: usize,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LongTermSearchResult {
    pub source_path: String,
    pub chunk_index: i32,
    pub content: String,
    pub score: f64,
}

pub async fn connect(url: &str) -> Result<LongTermMemory> {
    let (client, connection) = tokio_postgres::connect(url, NoTls)
        .await
        .context("failed to connect to pgvector database")?;

    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("pgvector connection error: {error}");
        }
    });

    Ok(LongTermMemory { client })
}

impl LongTermMemory {
    pub async fn init(&self) -> Result<()> {
        self.client
            .batch_execute(&format!(
                "CREATE EXTENSION IF NOT EXISTS vector;
                 CREATE TABLE IF NOT EXISTS agent_long_term_memory (
                    id BIGSERIAL PRIMARY KEY,
                    source_path TEXT NOT NULL,
                    chunk_index INTEGER NOT NULL,
                    content TEXT NOT NULL,
                    embedding vector({EMBEDDING_DIMENSIONS}) NOT NULL,
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                    UNIQUE(source_path, chunk_index)
                 );"
            ))
            .await
            .context("failed to initialize pgvector long-term memory schema")
    }

    pub async fn index_markdown_path(&self, path: &Path, chunk_chars: usize) -> Result<usize> {
        ensure!(chunk_chars > 0, "chunk_chars must be greater than zero");
        self.init().await?;

        let chunks = markdown_chunks_from_path(path, chunk_chars)
            .with_context(|| format!("failed to read markdown from {}", path.display()))?;
        let mut indexed = 0usize;

        for chunk in chunks {
            self.upsert_chunk(&chunk).await?;
            indexed += 1;
        }

        Ok(indexed)
    }

    async fn upsert_chunk(&self, chunk: &MarkdownChunk) -> Result<()> {
        let embedding = embedding_literal(&text_embedding(&chunk.content));
        let embedding = vector_cast_sql(&embedding);
        let chunk_index = i32::try_from(chunk.chunk_index).context("chunk index is too large")?;
        let source_path = chunk.source_path.to_string_lossy().to_string();
        let statement = format!(
            "INSERT INTO agent_long_term_memory
                (source_path, chunk_index, content, embedding, updated_at)
             VALUES ($1, $2, $3, {embedding}, now())
             ON CONFLICT (source_path, chunk_index)
             DO UPDATE SET
                content = EXCLUDED.content,
                embedding = EXCLUDED.embedding,
                updated_at = now()"
        );

        self.client
            .execute(&statement, &[&source_path, &chunk_index, &chunk.content])
            .await
            .context("failed to upsert markdown chunk into pgvector")?;

        Ok(())
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<LongTermSearchResult>> {
        ensure!(
            !query.trim().is_empty(),
            "long-term search query cannot be empty"
        );
        ensure!(
            limit > 0,
            "long-term search limit must be greater than zero"
        );
        self.init().await?;

        let embedding = embedding_literal(&text_embedding(query));
        let embedding = vector_cast_sql(&embedding);
        let limit = i64::try_from(limit).context("long-term search limit is too large")?;
        let statement = format!(
            "WITH query_terms AS (
                SELECT lower(term) AS term
                FROM regexp_split_to_table($2, '[^[:alnum:]]+') AS term
                WHERE char_length(term) >= 3
             ),
             ranked AS (
                SELECT source_path,
                       chunk_index,
                       content,
                       1 - (embedding <=> {embedding}) AS vector_score,
                       (
                           SELECT count(*)
                           FROM query_terms
                           WHERE lower(content) LIKE '%' || term || '%'
                       ) AS lexical_matches
                FROM agent_long_term_memory
             )
             SELECT source_path,
                    chunk_index,
                    content,
                    vector_score + lexical_matches::double precision AS score
             FROM ranked
             ORDER BY lexical_matches DESC, score DESC, vector_score DESC
             LIMIT $1"
        );
        let rows = self
            .client
            .query(&statement, &[&limit, &query])
            .await
            .context("failed to search pgvector long-term memory")?;

        Ok(rows
            .into_iter()
            .map(|row| LongTermSearchResult {
                source_path: row.get("source_path"),
                chunk_index: row.get("chunk_index"),
                content: row.get("content"),
                score: row.get("score"),
            })
            .collect())
    }
}

impl LongTermSearchResult {
    pub fn to_markdown(results: &[Self]) -> String {
        if results.is_empty() {
            return "No long-term memory results.".to_string();
        }

        let mut markdown = "Long-term memory context:".to_string();

        for (index, result) in results.iter().enumerate() {
            markdown.push_str(&format!(
                "\n\n{}. {}#{} (score {:.3})\n{}",
                index + 1,
                result.source_path,
                result.chunk_index,
                result.score,
                result.content
            ));
        }

        markdown
    }
}

pub fn markdown_chunks_from_path(path: &Path, chunk_chars: usize) -> Result<Vec<MarkdownChunk>> {
    ensure!(chunk_chars > 0, "chunk_chars must be greater than zero");

    let mut files = Vec::new();
    collect_markdown_files(path, &mut files)?;
    files.sort();

    let mut chunks = Vec::new();

    for file in files {
        let content = fs::read_to_string(&file)
            .with_context(|| format!("failed to read markdown file {}", file.display()))?;

        for (chunk_index, content) in split_markdown_chunks(&content, chunk_chars)
            .into_iter()
            .enumerate()
        {
            chunks.push(MarkdownChunk {
                source_path: file.clone(),
                chunk_index,
                content,
            });
        }
    }

    Ok(chunks)
}

fn collect_markdown_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        if is_markdown_file(path) {
            files.push(path.to_path_buf());
        }

        return Ok(());
    }

    ensure!(
        path.is_dir(),
        "{} is not a markdown file or directory",
        path.display()
    );

    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if is_markdown_file(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let extension = extension.to_ascii_lowercase();
            extension == "md" || extension == "markdown"
        })
        .unwrap_or(false)
}

fn split_markdown_chunks(content: &str, chunk_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for paragraph in content
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let separator = if current.is_empty() { "" } else { "\n\n" };
        let next_len =
            current.chars().count() + separator.chars().count() + paragraph.chars().count();

        if !current.is_empty() && next_len > chunk_chars {
            chunks.push(std::mem::take(&mut current));
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }

        current.push_str(paragraph);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn text_embedding(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0f32; EMBEDDING_DIMENSIONS];

    for token in text_tokens(text) {
        let hash = stable_hash(&token);
        let index = (hash as usize) % EMBEDDING_DIMENSIONS;
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }

    normalize_vector(vector)
}

fn text_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for character in text.chars() {
        if character.is_alphanumeric() {
            current.extend(character.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037u64;

    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }

    hash
}

fn normalize_vector(mut vector: Vec<f32>) -> Vec<f32> {
    let magnitude = vector.iter().map(|value| value * value).sum::<f32>().sqrt();

    if magnitude == 0.0 {
        return vector;
    }

    for value in &mut vector {
        *value /= magnitude;
    }

    vector
}

fn embedding_literal(vector: &[f32]) -> String {
    let values = vector
        .iter()
        .map(|value| format!("{value:.6}"))
        .collect::<Vec<_>>()
        .join(",");

    format!("[{values}]")
}

fn vector_cast_sql(embedding: &str) -> String {
    debug_assert!(
        embedding
            .chars()
            .all(|character| matches!(character, '[' | ']' | ',' | '.' | '-' | '0'..='9'))
    );

    format!("'{embedding}'::vector")
}

pub fn default_chunk_chars() -> usize {
    DEFAULT_CHUNK_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_markdown_into_bounded_chunks() {
        let chunks = split_markdown_chunks("one\n\nTwo words\n\nthree", 12);

        assert_eq!(
            chunks,
            vec![
                "one".to_string(),
                "Two words".to_string(),
                "three".to_string()
            ]
        );
    }

    #[test]
    fn detects_markdown_files() {
        assert!(is_markdown_file(Path::new("notes.md")));
        assert!(is_markdown_file(Path::new("notes.markdown")));
        assert!(!is_markdown_file(Path::new("notes.txt")));
    }

    #[test]
    fn embeddings_are_normalized_and_stable() {
        let first = text_embedding("Rust memory memory");
        let second = text_embedding("Rust memory memory");
        let magnitude = first.iter().map(|value| value * value).sum::<f32>().sqrt();

        assert_eq!(first, second);
        assert!((magnitude - 1.0).abs() < 0.001);
    }

    #[test]
    fn formats_search_results() {
        let results = vec![LongTermSearchResult {
            source_path: "notes.md".to_string(),
            chunk_index: 0,
            content: "Remember Valkey setup.".to_string(),
            score: 0.75,
        }];

        assert!(LongTermSearchResult::to_markdown(&results).contains("notes.md#0"));
    }
}
