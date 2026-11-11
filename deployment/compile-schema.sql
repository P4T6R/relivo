-- Empty, build-only schema for the upstream SQLx query macros.
-- Keep column types aligned with rustdesk-server/src/database.rs.
-- Never copied into runtime images or used as a deployed database.
CREATE TABLE peer (
    guid BLOB PRIMARY KEY NOT NULL,
    id VARCHAR(100) NOT NULL,
    uuid BLOB NOT NULL,
    pk BLOB NOT NULL,
    created_at DATETIME NOT NULL DEFAULT(CURRENT_TIMESTAMP),
    user BLOB,
    status TINYINT,
    note VARCHAR(300),
    info TEXT NOT NULL
) WITHOUT ROWID;
CREATE UNIQUE INDEX index_peer_id ON peer (id);
CREATE INDEX index_peer_user ON peer (user);
CREATE INDEX index_peer_created_at ON peer (created_at);
CREATE INDEX index_peer_status ON peer (status);
