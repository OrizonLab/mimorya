-- A document replaced by a new version of its file (ADR-150): the same
-- document, in the same space at the same sensitivity, its identifier kept
-- for the conversations that cite it; its file, name, title, kind, size,
-- pages and passages become the new version's, the former ones not kept.
-- `version` counts its files, so that a replacement names the one it
-- replaces and two people never overwrite each other unknowingly. Who
-- replaced it last, and when, stays with it; emptied when that account is
-- deleted, as its depositor is: the document stays the company's.
ALTER TABLE documents
  ADD COLUMN version integer NOT NULL DEFAULT 1 CHECK (version >= 1),
  ADD COLUMN replaced_at timestamptz,
  ADD COLUMN replaced_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  ADD CONSTRAINT documents_replaced CHECK ((version = 1) = (replaced_at IS NULL));

-- A document is otherwise never updated: only what a new file changes.
-- Its space, sensitivity, depositor and first deposit stay; its passages
-- are deleted and inserted again, as at a deposit.
GRANT UPDATE (title, file_name, kind, byte_size, page_count, version, replaced_at,
  replaced_by_user_id) ON documents TO mimorya_app;
GRANT UPDATE (content) ON document_files TO mimorya_app;
