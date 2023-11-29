# `mdbook-typst`

`mdbook-typst` is a
[backend](https://rust-lang.github.io/mdBook/for_developers/backends.html) for
[mdBook]. The backend converts the book to
[Typst] markup and can output any format Typst can (currently
`pdf`, `png`, `svg`, and raw Typst markup).

## Usage

First, install the Typst cli:

```sh
cargo install --git https://github.com/typst/typst
```

Next, install `mdbook-typst` (this project):

```sh
cargo install mdbook-typst
```

Finally, add an entry to your
[book.toml]:

```toml
[output.typst]
```

By default `mdbook-typst` will output raw Typst markup to `book/typst/book.typst`.

## Pdf and other formats

Pdf and other formats can be output instead of raw Typst markup. In your [book.toml] set the `format` value of the `output` config section:

```toml
[output.typst.output]
format = "pdf"
```

By default `mdbook-typst` will output to `book/typst/book.[format]`.

## Other configuration

`mdbook-typst` is fairly configurable. Check out [the configuration
code](./src/config.rs) for a complete list of options.

If you want more control, consider creating your own formatter and/or preprocessing the
book using the [pullup](https://github.com/LegNeato/pullup) project.

[mdBook]: https://github.com/rust-lang/mdBook
[book.toml]: https://rust-lang.github.io/mdBook/guide/creating.html#booktoml
[Typst]: https://typst.app/docs/
