# onepass

This is my CLI password generator. There are many like it, but this one is mine.

More specifically, this is my entry into the genre of deterministic password
generators: this app provides a way of turning a single master password into
any number of per-site passwords, allowing you to use unique, strong passwords
on all of your sites while only needing to memorize the one master password.
Besides this master password, no secret information is required; the config
file may specify per-site password schemas, but this can generally be stored
publicly.

## Installation

Release binaries are coming soon. For now, you can just use cargo:

```sh
cargo install onepass
```

Or build from source:

```sh
cargo build --release && install target/release/onepass ~/bin/onepass
```

A default config is generated at `${XDG_CONFIG_DIR:-$HOME/.config}/onepass/config.yaml`
on the first run of the program. You can also see my current config here:

<https://github.com/mrdomino/dotconfig/blob/main/onepass/config.yaml>

## Site representation

This app generates passwords for sites — or more specifically, for URLs. We use
absolute URLs, and at present we canonicalize to the result of Rust’s
`Url::parse` converted back into a string. If you specify a URL that does not
start with a scheme, we treat that as a `"https"` URL by default, i.e., we
prepend `https://` to what is passed. So e.g. if you run:

```sh
onepass google.com
```

we will use the full absolute URL `"https://google.com/"` (including path) as
the lookup key; likewise, a config entry for `"google.com"` will be mapped to
`"https://google.com/"`.

The default config shows an example for `iphone.local`; you might consider using
this to generate login passwords or PIN codes for your local devices. You could
also generate a password for a `mailto:` URL, or any other URL scheme.

Usernames are supported (for URL schemes which support them); you may specify a
username in your config file for a site. If you do, that username is prepended
to the URL, so the password will ultimately be for e.g.
`"https://user@example.com/"`.

## Schemas

You may specify a password schema to be used with a given site, either on the
command line or in the config file. These schemas may be specified in a regular
expression–like language; specifically, we support character classes, counts,
groups, and — as an extension to common regular expression syntax — words from a
word list. So:

- Character classes: `[A-Z]` for capital letters, `[A-Za-z0-9]` for alphanumeric
  characters.
- Counts: `[A-Z]{12}` for 12 uppercase letters.
- Groups: `([A-Za-z][0-9][A-Za-z]){3}` for 3 sequences of a letter, then a
  digit, then a letter, e.g. `"a0bc2d"`.
- Words: `[:word:]` will be replaced with a pseudorandomly chosen word from a
  configured word list. (`[:Word:]` is the same, but with the first character
  transformed to upper case.)

The config format also supports aliases for schemas; e.g. you may configure the
alias `"phrase"` to be `"[:word:]{4}"`, and then configure a site to use the
`"phrase"` schema.

### Changing passwords

It can happen that you wish to change your password for one site without wanting
to change all of your passwords, e.g. because of a password rotation policy or a
database compromise. We support this by way of a simple per-site increment
parameter; it starts at 0, and you can increase it by 1 to get a new password
for a given site.

## Implementation

We use argon2d to generate a 256-bit key from your master password, unique to
the site you requested. Specifically, we construct a salt as the following:

```
{increment},{url}
```

So e.g. for increment 0 (the default), if you run `onepass google.com`, then the
full salt will be:

```
0,https://google.com/
```

Then, we use that key to seed a BLAKE3 extendable output function, which we use
as an entropy source to generate a uniform random number between 0 and the
number of possible passwords matching the site’s configured password schema. We
then generate the password corresponding to that number and output it.

### Schemas

For the schema format, I decided not to include a few features of regular
expressions, mainly because they seemed of limited use for this application or
else seemed too difficult to use properly, e.g. without producing ambiguous
matches (i.e. multiple different ways of matching a given string, which would
correspond in our system to the same password being produced by multiple
numbers, which would compromise password security.) The main features I left
out were general choices (`a|b`) and quantifiers (`*`, `+`, and `?`.)

I obviously did not include non-regular constructions like backreferences, but
it would not be correct to say that these were “left out” of an implementation
of regular expressions. (The `[:word:]` extension is of course equivalent to a
choice between literals, e.g. `aardvark|bicycle|...|zoology`.)

## Acknowledgements

I was introduced to the idea of using deterministic passwords via [passacre][0]
(thanks pi), which I used for many years before switching to [lesspass][1] due
to certain macOS changes making it increasingly difficult to install passacre.

The idea to use regex-like password schemas came from [xfbs/passgen][2].

Recommendations on crypto primitives (BLAKE2 and Argon2 in particular) were due
to Justine Tunney.

The default embedded word list is from [the EFF][3].

## Limitations

This package uses 256-bit unsigned integers internally. As such, it does not
support passwords selected from universes containing more than `2**256` options,
and will probably break in strange ways on these. If you need more than 256
bits of entropy in your output passwords, this is probably not the package for
you.

## How I manage my passwords

I’ve been doing something like the following for the last decade or so; this may
perhaps serve as inspiration for your own approach to password management, or
else at least help explain why I found this app desirable enough to make it.

Every now and then when the mood strikes me, I get my hands on a good set of
casino dice. Using these dice and a diceware-style word list, I generate two
passwords consisting of some satisfying number of words. I commit both passwords
to memory. One of these becomes the password for my email account (which I can
use as a last resort to reset site passwords if my master password should ever
be compromised.) The other becomes my master password, from which all of my
other passwords are generated.

I use a deterministic password manager to generate all of my per-site passwords;
this has been lesspass for a while, and is hopefully going to be onepass going
forward. I then use my system or browser password manager as a cache for these
passwords, so that I don’t need to type in my master password that often.

I generally try to avoid typing my master password in public, or in places that
are likely to be surveilled.

[0]: https://github.com/habnabit/passacre
[1]: https://lesspass.com
[2]: https://github.com/xfbs/passgen
[3]: https://www.eff.org/dice
