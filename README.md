# onepass

This is my CLI password generator. There are many like it, but this one is mine.

This is an _extremely_ rough cut, but it should suffice to demonstrate the idea:
take a master password, shmush it together with a site URI, and sample it to get
a random number bounded by the size of your password schema’s universe.

This lets you not need to use a password manager, only need to memorize a single
password, and yet use unique passwords for all of your sites.

## Installation

For now just use cargo:

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

## Acknowledgements

I was introduced to the idea of using deterministic passwords via [passacre][0]
(thanks pi), which I used for many years until switching to [lesspass][1] due
to increasing infeasibility of installing passacre on newer versions of macOS.

The idea to use regex-like password schemas came from [xfbs/passgen][2].

Recommendations on crypto primitives (BLAKE2 and Argon2 in particular) were due
to Justine Tunney.

The default embedded word list is of course from [the EFF][3].

## Approach

We derive a key from your master password using argon2d, salted by your URL and
the site increment. (The site increment is just a number that starts at 0, that
you can increment every time you want to change your password on a site.) Then,
we use that key to seed a BLAKE3 extendable output function, which we use as a
source of entropy to generate a uniform random number between 0 and the number
of possible passwords matching that site’s configured password schema. We then
generate the password corresponding to that number and output it.

### Schemas

Your password schemas may be specified using a regular expression–like language.
E.g. you can say `[A-Z]{12}` to specify passwords consisting of 12 uppercase
letters, or `[A-Za-z0-9_-]{16}` for passwords consisting of 16 URL-safe base64
digits. We also support an extension to common regex syntax: `[:word:]` becomes
a random word from a user-configured dictionary, e.g. for use in passphrases.
(This is still a regular language; there is no reason save implementation
expediency that we could not treat `[:word:]` as just a group of choices of
different literals, one per dictionary word, e.g.: `(aardvark|bicycle|...)`.)

We decided not to include a few features of regular expressions, mainly because
they seemed of limited use for this application or else seemed too difficult to
use properly, e.g. without producing ambiguous matches (i.e. multiple different
ways of matching a given string, which would correspond in our system to the
same password being produced by multiple numbers, which would compromise
password security.) The main features we left out were general choices (`a|b`)
and quantifiers (`*`, `+`, and `?`.)

(We obviously did not include non-regular constructions like backreferences,
but it would not be correct to say that these were “left out” of a regular
expression implementation.)

### NFAQ

#### Why not just use the argon2d key directly?

Choosing a random number less than a bound, when the bound is not an even power
of 2, requires rejection sampling. As such, there is not a guarantee (beyond
statistical likelihood) on the amount of entropy we will need to consume in
order to produce such a number.

## Limitations

This package uses 256-bit unsigned integers internally. As such, it does not
support passwords selected from universes containing more than `2**256` options,
and will probably break in strange ways on these. If you need more than 256
bits of entropy in your output passwords, this is probably not the package for
you.

## OCD technical notes

This package uses blake3 instead of blake2, even though it also uses argon2,
which internally uses blake2. It would be simpler and involve fewer moving parts
cryptographically/mathematically if it could use [BLAKE2X][4], but I haven’t
found that in a crate yet and I haven’t wanted to hand-roll an implementation
of it. The reason I chose blake3 for this was solely that it already had an
off-the-shelf XOF in the crate.

I have not yet carefully vetted the way that `RngCore` and `crypto-bigint`
construct random numbers, and do not yet know if their choices around things
like endianness are agreeable to me.

[0]: https://github.com/habnabit/passacre
[1]: https://lesspass.com
[2]: https://github.com/xfbs/passgen
[3]: https://www.eff.org/dice
[4]: https://www.blake2.net/blake2x.pdf
