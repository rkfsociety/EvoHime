# Wave 4: Project Context 2.0 (7.112)

**Goal**: Enhance semantic search and retrieval quality with embeddings-based indexing

**Timeline**: ~1 week (3 phases)

## Current State
- Lexical search via ripgrep (fast, precise for keywords)
- Chunk-based results with line ranges
- Path/symbol/content weighting
- Max 5 results, 256KB per file limit

## Limitations
- No semantic understanding (can't find conceptually similar code)
- Simple weighting schema (not learned)
- No embeddings caching/dedupe
- Chunking breaks long symbol definitions

## Proposed Architecture

### Phase 1: Embeddings Foundation
- Embed chunk text using local model (e.g., all-MiniLM-L6-v2)
- Cache embeddings in `embedding_cache.db` (SQLite)
- Version control for model changes
- Dedupe identical chunk hashes

### Phase 2: Semantic Search
- Hybrid search: lexical + semantic scoring
- Combine BM25 + cosine similarity
- Rerank results by combined score
- Expose `search_semantic()` method

### Phase 3: Retrieval Optimization
- Adaptive chunk sizing based on symbol type
- Symbol-aware weights (functions > variables > comments)
- Path hierarchy weighting (adjacent files boost)
- Relevance feedback loop

## Key Decisions

### Embedding Model
- **all-MiniLM-L6-v2** (22.7MB, 384-dim, fast)
- Alternative: ONNX Runtime for inference
- Fallback: lexical if GPU unavailable

### Caching Strategy
- SQLite for persistence
- Hash-based dedup (`SHA256(chunk_text)`)
- Lazy loading on search
- Versioned with model hash

### Scoring Formula
```
final_score = 0.4 * bm25_score + 0.6 * cosine_similarity
rerank_multiplier = path_weight * symbol_type_weight
```

## Files to Modify
- `crates/project-index/Cargo.toml`: Add embedding deps
- `crates/project-index/src/lib.rs`: Semantic search methods
- `crates/project-index/src/embeddings.rs`: New embedding module
- `crates/project-index/src/cache.rs`: New caching module
- `crates/server/src/`: Integration points

## Success Criteria
- [ ] Semantic search finds similar code by concept
- [ ] Embedding cache reduces 2nd+ search latency by 90%
- [ ] Dedupe eliminates redundant embeddings
- [ ] Path weighting improves relevance ranking

## Not in Scope
- Remote embedding service
- Fine-tuning on codebase
- Vector database (SQLite sufficient)
- Async embedding computation
