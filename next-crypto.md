# Crypto revision ideas

Some of these are breaking changes (and would necessitate a v4; not currently planned), but some of them, and some parts of some of them, could be done in v3.

## 1. Two-stage seed derivation

At present, we store the seed passphrase directly in the OS keyring, and do the full derivation from seed on every run. This has two problems:

1. The seed itself is at risk of exposure if there is a bug in onepass.
2. The Argon2 computation _per site_ must be sufficiently expensive to preclude offline attacks, or else the seed can be brute-forced from any sufficiently complex site password. For v3, we have chosen pretty aggressive parameters and not made these customizable, so this slows down usage noticeably on older or embedded devices, makes batch operations more challenging, and may preclude usage altogether in some memory- or compute-bound scenarios.

Instead, we could do one derivation step from the seed passphrase to an initial secret, store that secret, and do per-site derivations against that secret, to provide a layer of security against the input seed without sacrificing as much online performance cost.

## 2. Better salt

At present, the per-site Argon2 salt is a function of the site configuration: URL, username, schema, and increment. But of this information, only the username varies per user of onepass, and it varies in an easily guessable way. Moreover, onepass has supported operation without specified usernames; this makes it so that a rainbow table for the null username may be constructed on any given `(URL, schema, increment)` to attack _all_ onepass users for a given site. We would much prefer for attackers to have to expend that cost per user instead of expending it per site.

Setting the username seems like a reasonable mitigation for this for now; perhaps a default username can be introduced, and a mechanism for explicitly setting the null username (currently this is the same as `username=""`.)

A per-site (or even global) `salt` parameter would do even better here and is more obviously defensible from a cryptographic perspective. However, it would do better at the cost of sacrificing one of the design properties I originally intended for onepass: that site passwords should always be reconstructible from publicly available information plus the secret seed phrase, even if the config is somehow lost. Making this change would change the config from essentially a cache/memory layer into a crucial component of the password generator. It may be worth introducing it as an option for some users.

### 2a. Pepper

If we are crossing the bridge into storing data beyond the seed passphrase, we could also consider introducing “pepper,” cryptographic material stored in hardware or encrypted on disk. This would make it so that the seed passphrase was no longer the weak link in the chain, i.e., even a user who picked a very easily guessable seed passphrase would still be protected by the pepper, but it would do so at the cost of adding another secret that A, must be synchronized, and B, can be lost, rendering all site passwords inaccessible.

## 3. Hashed/encrypted config files

At present, configs leak metadata about which URLs a user has passwords saved with. This is presently only mitigated by `include` directives, which allow a user to import a (say `gitignore`d) local config with extra URLs in it. We could mitigate it more generally by supporting encrypted configs (maybe with the seed passphrase or a secret derived therefrom), and/or by hashing site URLs, so rather than doing a tree lookup we would do a hash lookup. Hashed URLs are a bit trickier to use (was this password saved on `google.com` or `accounts.google.com` or `google.com/login`?), but this may be an acceptable tradeoff for some users.
