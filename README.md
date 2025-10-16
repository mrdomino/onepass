# onepass

Onepass is a deterministic CLI password manager. Think of it like an ordinary password manager (à la 1Password or the macOS Passwords app) except that instead of needing to store and keep track of every single password it generates, you only need to keep track of the one seed password from which the rest are grown.

Individual site passwords can be cycled without changing your seed password by increasing a per-site “increment” parameter. Different password schemas are supported per site, such as “one lowercase letter, one uppercase letter, one digit, 15 alphanumeric characters.” The specification for these schemas is a regex-like syntax, and schemas may also be mapped to human-readable aliases; the [example config](example/config.yaml) demonstrates how this works.

Optionally, you may sync your seed password to the system keyring to protect it from being shoulder-surfed as you’re entering it. On macOS, this mode requires TouchID (or other biometric auth) to unlock your seed password; since these APIs require paid developer certificates to work, the prebuilt macOS binaries are signed with one.

Keyring sync can be requested either with the `-k` / `--keyring` CLI arg, or the `use_keyring: true` config setting.

## Install from GitHub releases
Download the [latest GitHub release](https://github.com/mrdomino/onepass/releases/latest) binary for your platform, or download and run the `onepass.pkg` installer for recent (i.e. Apple Silicon running OS X 10.13+) macOS machines.

On macOS, because we only ship a binary and not a full app bundle, you will also need to download and install the [included provisioning profile](https://github.com/mrdomino/onepass/raw/refs/heads/main/onepass.provisionprofile) or the app will be killed by GateKeeper on startup. At present, this step must be done manually.

## Quick start
Simply call `onepass` with the URL or hostname of the site for which you’d like to generate a password:
```sh
onepass google.com
```

You can override site settings with either config file entries or command-line flags. E.g. to use a password schema consisting of 18 alphanumeric characters:
```sh
onepass google.com -s '[A-Za-z0-9]{18}'
```

A default config file is generated at `${XDG_CONFIG_DIR:-$HOME/.config}/onepass/config.yaml` on the app’s first run. You may customize this config file; as it does not contain any sensitive data, you may like to back it up with other non-sensitive documents.

## Other installation methods

### From cargo
Onepass may be installed via cargo:
```sh
cargo install onepass
```

Note, however, that on macOS, the biometric keyring support will not be enabled.

### From source
```sh
cargo build --release &&
  sudo install target/release/onepass /usr/local/bin/onepass
```

To enable the macOS biometric keyring support, you will need to produce a codesigned binary.

To do this, you will probably need to edit [`onepass.entitlements`](data/onepass.entitlements) to replace the team ID with your own.

Assuming you’re using an Apple Development local-only signing key, you should be able to do something like the following:
```sh
sed "s/2TM4K8523U.org.whilezero.app.onepass/$MY_TEAM_ID.*/" \
    data/onepass.entitlements > my-onepass.entitlements &&
  cargo build --no-default-features --features macos-biometry --release &&
  codesign \
    --force \
    --options runtime \
    --entitlements my-onepass.entitlements \
    --sign "Apple Development" \
    target/release/onepass
```

If it worked, you should be able to run `target/release/onepass -k google.com` and the command should succeed after reading your seed password; on a second run, or if there is already a password saved, you should see a TouchID prompt and not need to reenter your seed password.

## How it works
We take your seed password as input, as well as the URL of the site for which you are requesting a password. This gets passed through a [key derivation function](https://en.wikipedia.org/wiki/Key_derivation_function) to turn it into a unique, hard-to-guess number corresponding to your password for that site. We then find the password corresponding to that number within the schema you requested; e.g. if your schema is `[1-9]` (which you shouldn’t ever actually use, because it would be far too weak) and the number corresponds to the 6th password, then your password for that site will be `"6"`. Or if your schema is `[a-zA-Z0-9_-]{8}`, i.e. 8 base64 digits, then your password will be somewhere between `aaaaaaaa` and `--------` depending on what number is derived.

As long as your seed password remains secure, nobody else can guess these generated passwords. Because the key derivation function gives the same outputs for the same inputs every time, all of your passwords always come out the same way, so none of them need to be stored or remembered; they can just be recomputed every time you need them. This takes the secret storage requirements down from O(_n_) where _n_ is the number of passwords you have down to O(1), your seed password.

Crucially, this reduces the inertia with any specific password manager; it is far easier to move one password around, that you’ve probably memorized, than it is to move tens, hundreds, or thousands of passwords. Maybe this is why none of the major providers work this way.

Note that you do not need to stop using your existing password manager under this scheme; you may keep using Google or macOS Passwords, or any other traditional password manager, alongside onepass. In fact this is recommended, both for convenience and security reasons — by being integrated into the browser, these apps can often check that the site URL you are visiting is the correct one, defeating some phishing attacks.

But when you register for a new site, instead of taking the default password generated by your other password manager, you can instead get the password from onepass and then store _that_ in your password manager. Then if you ever decide to change providers (say, leaving Apple’s or Google’s ecosystem), you do not need to migrate all of your passwords and can instead get them back out of onepass.

(It is of course true that there are storage requirements beyond just the seed password: you also need to keep track of what site uses which schema, and what the current increment of each site is. But none of that state needs to be secret; an attacker can view it all and still not be able to guess your passwords. You could take this configuration and email it to yourself, say, or put it in iCloud or Google Drive.)

### Details
The key derivation function we use is [Argon2id](https://en.wikipedia.org/wiki/Argon2). (We use Argon2id because in some cases, it might be feasible for an attacker to measure the time it takes for `onepass` to run without otherwise being able to access the seed password.) This gives us a 256-bit secret unique to the site and your seed password; we use this to seed a [ChaCha20](https://en.wikipedia.org/wiki/Salsa20#ChaCha_variant) stream cipher, giving us an unbounded sequence of cryptographic pseudorandom bits. We use this sequence to select a random number between 0 and N, where N is the number of different passwords the given schema can generate. (Why not just use the Argon2id output itself? Because to generate a random number uniformly with an arbitrary bound requires [rejection sampling](https://en.wikipedia.org/wiki/Rejection_sampling).)

All of the above lives under [src/crypto.rs](src/crypto.rs) and the relevant packages. The other piece involves measuring and indexing password schemas. This functionality lives under [src/randexp.rs](src/randexp.rs). We define a custom regex-like language, deliberately excluding some features (namely wildcards and choices) that would either give us cardinality troubles or make it too easy for schemas to contain aliases, i.e. multiple different numbers corresponding to the same password. (For a trivial example: `[ab]|a` can generate the password `"a"` in two different ways: either picking the first choice and then selecting `'a'` or picking the second choice. We don’t want these because they undermine our guesses at the amount of entropy in a password.) We then define, for expressions in this language, a way of counting the number of passwords that that expression can generate; this gives us our upper bound N. And finally we define a way of translating numbers into passwords that match the given expression; this completes the password generator.

## Support
This is a side project about which I am considering becoming more ambitious. To help me out or signal interest in me investing further effort, consider doing one or more of the following:
* Star the repo
* Tip me: <https://ko-fi.com/mrdomino>
* [Tell me how you like it](mailto:onepass@whilezero.org?subject=Thanks+for+making+onepass!+Some+feedback…)

## Acknowledgements
I initially came across deterministic password generators in 2013 via [habnabit/passacre](https://github.com/habnabit/passacre), which I happily used for several years until macOS had drifted to the point of making it prohibitively difficult to install. I then transitioned to [lesspass](https://lesspass.com/).

The idea for the regex-like schema syntax was due to [xfbs/passgen](https://github.com/xfbs/passgen).
