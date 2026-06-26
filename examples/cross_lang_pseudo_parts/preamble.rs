// examples/cross_lang_pseudo.rs
// Test: same function in all 11 languages → pseudocode
// Run: cargo run --example cross_lang_pseudo
//
// Validates the embedding hypothesis:
// If pseudocode of the same function across languages looks similar,
// embeddings will cluster them correctly — confirming the fine-tuning approach.

use anchor::parser::language::SupportedLanguage;
use tree_sitter::{Node, Parser};

// ── Same function in every language ──────────────────────────────────────────
// find_user(id): query db, return user if active, else null

const RUST: &str = r#"
pub fn find_user(id: u64) -> Option<User> {
    let user = db.query(id);
    if user.is_none() {
        return None;
    }
    if user.active {
        return Some(user);
    }
    None
}
"#;

const PYTHON: &str = r#"
def find_user(id):
    user = db.query(id)
    if user is None:
        return None
    if user.active:
        return user
    return None
"#;

const GO: &str = r#"
func findUser(id uint64) *User {
    user := db.Query(id)
    if user == nil {
        return nil
    }
    if user.Active {
        return user
    }
    return nil
}
"#;

const JAVASCRIPT: &str = r#"
function findUser(id) {
    const user = db.query(id);
    if (user === null) {
        return null;
    }
    if (user.active) {
        return user;
    }
    return null;
}
"#;

const TYPESCRIPT: &str = r#"
function findUser(id: number): User | null {
    const user = db.query(id);
    if (user === null) {
        return null;
    }
    if (user.active) {
        return user;
    }
    return null;
}
"#;

const JAVA: &str = r#"
public User findUser(long id) {
    User user = db.query(id);
    if (user == null) {
        return null;
    }
    if (user.active) {
        return user;
    }
    return null;
}
"#;

const CSHARP: &str = r#"
public User FindUser(long id) {
    var user = db.Query(id);
    if (user == null) {
        return null;
    }
    if (user.Active) {
        return user;
    }
    return null;
}
"#;

const RUBY: &str = r#"
def find_user(id)
    user = db.query(id)
    if user.nil?
        return nil
    end
    if user.active
        return user
    end
    nil
end
"#;

const CPP: &str = r#"
User* findUser(uint64_t id) {
    User* user = db.query(id);
    if (user == nullptr) {
        return nullptr;
    }
    if (user->active) {
        return user;
    }
    return nullptr;
}
"#;

const SWIFT: &str = r#"
func findUser(id: UInt64) -> User? {
    let user = db.query(id)
    if user == nil {
        return nil
    }
    if user!.active {
        return user
    }
    return nil
}
"#;

// ── Pseudocode emitter ────────────────────────────────────────────────────────

struct Emitter<'a> {
    src: &'a [u8],
    lang: SupportedLanguage,
    out: String,
    indent: usize,
}

