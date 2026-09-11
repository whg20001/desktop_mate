# Memory ownership and provider contracts

SQLite `memory-events.sqlite3` owns the approved facts, pending approvals,
source references, revisions, extraction receipts, and index delivery jobs.
Mem0 and optional Graphiti are derived indexes. Replacing an index must not
change approval, deletion, scope isolation, or conversation retry semantics.

## Writing and correction

The conversation and pending extraction flag are committed together in the
conversation database. A single worker performs background extraction. Approved
or pending candidates, provider jobs, and the processed-turn receipt are committed
in one transaction in the memory database. If the process exits before marking
the conversation flag complete, the receipt prevents a second extraction, even
if the model would produce different wording. Empty/rejected batches also get
receipts. Receipts contain scope and turn IDs, not the original conversation.

The shared policy requires an exact quote from a user message, `sourceRole=user`,
and finite confidence of at least 0.7 for newly extracted facts. Local inference
also validates these fields. This verifies attribution, not semantic entailment:
model interpretation still needs evaluation against real conversations.

An extractor may propose `metadata.supersedes` using only the IDs supplied in
`existing_memories`. The manager restricts replacements to semantic facts in the
same scope and records their observed revisions. Automatic approval retires the
old fact in the new fact's transaction; when confirmation is enabled, retirement
waits for approval. Approval fails if the referenced version changed. Reject that
stale proposal and ask for a new correction based on the current fact.

Semantic deduplication uses normalized content and kind. Episodic deduplication
also requires the same occurrence time and source-turn list. Different dates or
different source events remain separate, including during result fusion.

## Retrieval

The local fallback tokenizes Latin words and Chinese character bigrams, removes
common query terms, and only returns records with lexical overlap. Mem0 uses a
minimum similarity score of 0.35; this is a starting threshold, not a benchmarked
universal cutoff for all embedding models.

Provider hits are mapped to canonical IDs and validated for scope, approval and
revision/content. Graphiti hits expose their source episode IDs; only synchronized
canonical sources may be recalled. Results contain the current canonical facts,
not unchecked graph-generated text. Rankings are fused by canonical ID with
reciprocal rank fusion (k=60), rather than comparing incompatible provider scores.
The manager rechecks revisions after retrieval to exclude concurrent edits/deletes.

## Adding or replacing a framework

Implement `LongTermMemoryProvider` in `memory.py` and select it in
`create_providers`. Rust accepts syntactically valid provider identifiers; the
Python manager verifies the requested provider is actually enabled.

The adapter contract is:

- `search(query, scope, limit)` returns ranked candidate dictionaries. Each hit
  supplies `id`, `content`, and either `metadata.canonicalEventId` plus `revision`,
  or `canonicalEventIds` for graph source episodes. Old record IDs can also be
  resolved through persisted delivery mappings. Unknown hits are not recalled.
- `upsert(event, provider_record_id)` preserves scope and canonical ID, refreshes
  metadata including revision, and returns a stable/recoverable provider ID.
- `delete_record(id, scope)` is scoped and idempotent, including when a crash
  prevented persisting the provider ID. It must raise on an actual service failure.
- `list(scope)` exposes legacy records for migration; it must enforce scope.
  Records without canonical linkage are imported as pending approval, with a
  stable ID and a receipt. Deleted or rejected imports cannot be re-imported by
  refreshing the page. Their external copies are scheduled for removal.
- `ready()` and `close()` report dependency state and release owned resources.

For a replacement *extractor*, implement `MemoryInferenceProvider.extract`,
including `existing_memories`, and return `MemoryCandidate` values with the same
evidence metadata. The shared policy still applies. Do not enable a framework's
independent fact mutation against the index without propagating proposed changes
through the canonical store and approval policy.

Mem0 is pinned to 1.0.11. Local-only HTTP client and telemetry integration remains
inside the adapter. Upgrades should pass the adapter contract tests and the real
Mem0/embedded-Qdrant test, which mocks model calls and uses temporary data.

## Background work, recovery and deletion

The worker prioritizes index deliveries (deletes first) and processes at most one
extraction per cycle. A busy Mem0 index does not hold up conversational fallback
retrieval. An already running model call is not preempted; background extraction
and conversation can still contend at the local model server.

Settings displays extraction/index queue counts and failures, worker state, and
a retry button. Retry clears due times for the selected user/character and can
restart a failed worker. Disabled writes remain disabled. Unexpected worker
failure leaves canonical data available for inspection until retry or shutdown.

Deleting a long-term memory excludes it from new recall immediately, while
physical index removal completes asynchronously. It does not delete original
chat history, Mem0 audit history, backups, or content already sent to an in-flight
LLM request. The retention setting only purges eligible deleted/rejected canonical
records; it is not a whole-conversation erasure policy.

Existing approved canonical facts are preserved. Untracked legacy index records
require approval before recall. No migration is run against user data by tests;
schema additions are applied on the next normal application start.

## Validation

`pnpm check` covers frontend types and UI failure/retry behavior.
`python -m unittest discover -s brain-sidecar/tests` (with `brain-sidecar/src` on
PYTHONPATH) covers evidence, Chinese relevance, stale/foreign hits, corrections,
dated episodes, transactional rollback, replay, legacy migration, worker recovery,
HTTP authorization, and the installed Mem0 adapter.
`cargo test --manifest-path src-tauri/Cargo.toml --offline` checks the desktop host.
