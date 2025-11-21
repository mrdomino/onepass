# Cryptosystem change ideas

This document collects ideas for breaking changes to the password generation scheme. these may make it into the next major version.

## Include the schema in the salt
By adding the schema to the salt explicitly, we guarantee that different password schemas will generate uncorrelated passwords from each other. That it doesn’t work that way right now is a little unintuitive; users will expect that a change of schema will be a new password, and there is no reason to preserve the incidental correlation across different schemas.

This will require somehow having a way to distinguish the end of the schema, probably by escaping or encoding the schema (unless we can use a delimiter that will not ever appear in a URL.)

# Configuration changes

## Multiple usernames, schema per username

## Toml instead of YAML

We are using YAML because it is not easy to use URLs as keys in toml files, but we could get around this with toml arrays:
```toml
[[site]]
url="url"
schema="schema"
```
