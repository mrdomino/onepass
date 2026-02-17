# onepass

Onepass is the canonical free/open-source CLI for the onepass deterministic password generator.

## Background

_[Em-dashes my own.]_

Why would you want a deterministic password generator? It sounds kind of like a solar-powered flashlight — isn’t the whole point of a password generator to be random, i.e. nondeterministic? Well, no: the point is to generate passwords that cannot be feasibly guessed by an attacker. Onepass just approaches this problem somewhat differently from a traditional password manager.

A traditional password manager, when it generates passwords, generally tries very hard never to generate the same password twice — in other words, it tries to be nondeterministic. But this then creates a problem: these nondeterministically generated passwords must be stored somewhere.

To solve this problem, a whole market sector has sprung up around password _managers_ as distinct from password _generators_. These password managers often charge ongoing subscription fees to store all of the sensitive, private data they produce; the data being private and sensitive, they also do not typically make it very easy to export it, say to migrate from one password manager to another. This contributes to the moat around browsers, or around operating systems, or around different SaaS password manager applications, allowing them to charge you more or make your experience worse before it is worth your time and energy to leave. (Not all of them do this! There are some nice password managers out there. But they all _can_ do this.)

Onepass takes a different approach: when it generates passwords, it tries very hard to _always_ generate the _same_ password _for the same site_, from one seed password. The system ideally involves no nondeterminism whatsoever, aside from whatever process generates the seed password. Instead, we rely on cryptographic algorithms to make it infeasible to guess one site password from another, or to guess the seed password from a site password. By doing this, we eliminate the problem that the password management sector is trying to solve: instead of needing to store, protect, and synchronize hundreds or thousands of different sensitive, private passwords, you only need to store _one_ piece of private data: the seed. All the rest of your passwords are generated on the fly as you need them, and needn’t ever be stored at all.

## Installation from GitHub releases

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

A default config file is generated at `${XDG_CONFIG_DIR:-$HOME/.config}/onepass/config.toml` on the app’s first run. You may customize this config file; as it does not contain any sensitive data, you may like to back it up with other non-sensitive documents.

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

To enable the macOS biometric keyring support, you will need to enable the `macos-biometry` feature and produce a codesigned binary.

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

## FAQs

### Should I use onepass?

Generally, no. Onepass is a tool for power users; if you are in the minority of people who feels comfortable typing commands in a CLI or installing software from GitHub releases, then yes, maybe. But this is designed to be a reference implementation, not a complete password management solution. There are many large, unanswered questions here; e.g.:

1. There is no graphical user interface.
2. There is no automatic mechanism to synchronize your configuration between different devices. (It can be stored in [dotfiles](https://github.com/mrdomino/dotconfig/blob/main/onepass/config.toml) — if you are the sort of person who is comfortable versioning dotfiles.
3. There is no way to migrate from a different password manager to onepass, aside from changing all of your passwords on all of your sites all at once.
4. There is no automated export from onepass to any other format or any other password manager. (This can be scripted — if you are the sort of person who is comfortable writing scripts.)
5. For its security, onepass depends absolutely and crucially on the seed password being a strong, securely generated password, and does nothing whatsoever to ensure this.

If you understand the above and it is not off-putting to you, then yes, you might should use onepass. If not, consider [supporting me](#support) in building a more user-friendly interface.

### How should I generate my seed password?

The author uses and recommends [Diceware](https://theworld.com/~reinhold/diceware.html), ideally with actual, casino-grade dice. The [EFF Dice-Generated Passphrases](https://www.eff.org/dice) site may also be helpful. Do not use a weak password for your seed; you only need one, so invest some time into it.

### How do I remember my site passwords?

Generally you’ll want to store these in onepass’s configuration file, so you know exactly what settings you used, so the password you generate for that site stays the same over time. Onepass may someday attempt to be better about recording this automatically; for now, it must be done manually.

### What if I need to change my password on just one site?

Onepass site passwords include an “increment” parameter. This is just a number; it starts at zero, and you can increase it any time you want a new password for that site. The number itself is not private; it does say when you’ve rotated a site password, but does not reveal anything about what your old or new passwords are.

### What do I do about different password requirements?

One of the core components of onepass is a language for describing site password schemas. If you are familiar with regular expressions (as from e.g. JavaScript or re2), this is similar; e.g. `[0-9a-z]{16}` says “16 lowercase letters or digits.” The difference is that whereas regular expressions are mainly used to search for patterns in text, these expressions are used to generate text that matches patterns.

If a site says that your password can only be up to 20 characters and must contain a digit, a lowercase letter, and an uppercase letter, you could use the following schema: `[0-9][a-z][A-Z][[:print:]]{17}`. This generates a password whose first character is a digit, second character is a lowercase letter, and third character is an uppercase letter, followed by 17 characters that could be any printable character, for 20 characters total. (Pedants of a certain type may note that this schema is a bit overconstrained; e.g. the requirements are just that the password _contains_ a digit, not that it _starts with_ a digit. So it is for now; this password still has about 125 bits of entropy, so we accept the slight loss.)

### Can I use this for things other than website passwords?

Yes. The URL field can be anything. You may wish to use the `.local` internal-only domain; e.g. `my-laptop.local` can be a login password for your laptop. You may wish to use a schema like `[0-9]{8}` to generate an 8-digit PIN for a phone’s lock screen.

At present, the app does not support generating cryptocurrency seed phrases, which involve a bit more structure than just “twelve arbitrary words,” but contributions to add support are welcome and should not be overly difficult.

## Support

If you like this project, consider doing one or more of the following:
* Star the repo
* Tip me: <https://ko-fi.com/mrdomino>
* [Tell me about it](mailto:onepass@whilezero.org?subject=Thanks+for+making+onepass!+Some+feedback…)

## Acknowledgements

I initially came across deterministic password generators in 2013 via [habnabit/passacre](https://github.com/habnabit/passacre) (thanks to [atax1a](https://infosec.exchange/@atax1a)). I happily used passacre for several years until macOS had drifted to the point of making it prohibitively difficult to install. I then transitioned mainly to [lesspass](https://lesspass.com/).

The idea for the regex-like schema syntax was due to [xfbs/passgen](https://github.com/xfbs/passgen).

Randall Bosetti helped in many ways: with learning Rust, with designing the subset of regular expressions used here, and with refining my taste by responding thoughtfully to various half-baked ideas.

Initial input on the primitives used for the cryptosystem came from Justine Tunney.
