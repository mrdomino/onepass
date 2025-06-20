# Hacking onepass

Technical details for the curious.

## Architecture

onepass is built around three core modules:

- **config** — YAML parsing and site lookup
- **randexp** — Regular expression engine for password schemas
- **url** — URL canonicalization

The main flow: parse config → canonicalize URL → derive key → generate password.

## Password Generation Algorithm

### Step 1: Salt Construction
```
salt = "{increment},{canonical_url}"
```
Example: `"0,https://google.com/"`

### Step 2: Key Derivation
```rust
let argon2 = Argon2::default();
argon2.hash_password_into(master_password, salt, &mut key_material);
```

### Step 3: Pseudorandom Generation
```rust
let mut hasher = blake3::Hasher::new();
hasher.update(&key_material);
let mut rng = Blake3Rng(hasher.finalize_xof());
```

BLAKE3’s extendable output function (XOF) gives us a cryptographically secure, unlimited stream of pseudorandom bytes.

### Step 4: Password Selection
```rust
let index = U256::random_mod(&mut rng, &NonZero::new(schema_size).unwrap());
let password = words.gen_at(&schema, index)?;
```

We generate a uniform random number in the range `[0, schema_size)` and use it to select the password at that index from all possible passwords matching the schema.

## URL Canonicalization

URLs get normalized to ensure consistency:

1. **Scheme Addition**: `google.com` → `https://google.com`
2. **URL Parsing**: Uses Rust’s `url` crate for RFC-compliant parsing
3. **Username Injection**: If specified, username gets added: `https://user@example.com/`
4. **Serialization**: Canonical string representation

This means `google.com`, `https://google.com`, and `https://google.com/` all generate the same password (they all canonicalize to `https://google.com/`).

## Schema Language

Our regex-like schema language supports a subset of regular expressions chosen to avoid ambiguity in password generation:

### Supported Features
- **Character classes**: `[A-Z]`, `[0-9]`, `[a-zA-Z0-9]`
- **Repetition**: `{n}` for exactly n repetitions
- **Groups**: `(pattern)` for grouping
- **Word lists**: `[:word:]` (lowercase) and `[:Word:]` (capitalized)

### Deliberately Unsupported
- **Alternation**: `a|b` — could easily create ambiguous matches
- **Quantifiers**: `*`, `+`, `?` — same ambiguity problem (and also some very minor cardinality problems)
- **Backreferences**: not regular, PCRE this is not

### Why These Restrictions?

Each password corresponds to exactly one number in the range `[0, schema_size)`. If multiple regex matches could produce the same string, we’d have multiple numbers generating identical passwords, which would reduce entropy and compromise security.

## Schema Enumeration

The `randexp` module implements a custom enumeration algorithm:

```rust
trait Enumerable {
    fn size(&self) -> U256;  // Total possible matches
    fn gen_at(&self, index: U256) -> String;  // Generate match at index
}
```

This lets us:
1. Calculate total entropy: `log₂(schema_size)` bits
2. Directly generate the password at any index without enumerating all possibilities

## Cryptographic Choices

**Argon2id**: Memory-hard key derivation. We use default parameters.

**BLAKE3**: Successor to BLAKE2, with a nicer (and built-in) XOF.

**256-bit arithmetic**: All internal calculations use 256-bit unsigned integers, supporting password universes up to `2**256` possibilities.

## Word Lists

Default word list is the [EFF’s large word list](https://www.eff.org/dice) — 7776 words designed for diceware. Custom word lists supported via `--words` flag or the `words_path` config setting.

## Configuration

Config file format (YAML):
```yaml
default_schema: login      # Default schema for sites
aliases:                   # Schema shortcuts
  pin: '[0-9]{8}'
  phrase: '[:word:](-[:word:]){4}'
sites:                     # Per-site overrides
  example.com:
    schema: phrase
    increment: 2
    username: myuser
```

Sites can be specified as:
- Simple string: `"example.com: mobile"`
- Full object with schema/increment/username overrides

## Error Handling

We use `anyhow` for error handling throughout. Key error cases:
- Invalid schemas (parsing failures)
- Missing config files (auto-generated on first run)
- Schema overflow (>2²⁵⁶ possibilities)
- Password confirmation mismatches

## Build Process

### Local Development
```sh
cargo build
cargo test
```

### Release Builds
GitHub Actions handles multi-platform builds:
- **Linux**: `x86_64-unknown-linux-gnu`
- **macOS**: `aarch64-apple-darwin`, `x86_64-apple-darwin`

macOS binaries get code-signed and notarized automatically.

Some day we might try to ship Windows binaries or an app; ask nicely.

## Security Considerations

**Master password**: Never stored or logged. Immediately zeroized after use.

**Key material**: All cryptographic material uses `Zeroizing` types to clear memory on drop.

**Side channels**: Argon2id parameter choice assumes CLI usage where side-channel attacks are impractical.

**Password rotation**: Increment parameter allows site-specific password changes without master password changes.
