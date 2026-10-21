# Generate PDF

This component makes a PDF file from an HTML template. It renders the HTML as a
[Liquid](https://shopify.github.io/liquid/) template with the selected variables. It makes the
PDF with [fullbleed](https://crates.io/crates/fullbleed) inside the component. It stores the PDF
in a file property of a model. It returns the file reference.

This component is the native version of the block-store function `generate-pdf` 2.0, which uses
LiquidJS:

- The variables arrive as JSON. Each variable is available by its key, as in 2.0.
- A variable or a property that does not exist is an error. The step fails with the message
  `Unknown variable` or `Unknown index`. See [Decisions](#decisions).
- A selected variable that has no value renders as empty text.
- The `group: "key"` filter groups a collection in the same way as lodash `groupBy`. The result
  is a list of `[value, items]` pairs in input order. A `{% for group in groups %}` loop iterates
  this list, as in 2.0. You cannot get one group by its name (`groups.some-value`).
- Only the Liquid standard filters are available. The LiquidJS extra filters are not available.
- The component adds `.pdf` to the file name when the name does not end in `.pdf`.

## Rendering

fullbleed renders HTML and CSS to PDF without a browser. The result is not identical to the
result of Chromium. fullbleed cannot download resources. The code in `src/pdf/` downloads the
resources before fullbleed renders the document:

- The component downloads linked stylesheets and `@import` rules and inlines them. The maximum
  import depth is four.
- The component downloads the fonts in `@font-face` rules and embeds them. TTF, OTF, WOFF and
  WOFF2 fonts are accepted. The `embedded-opentype` and `svg` candidates are skipped. Only the
  first readable candidate of each `src` list is downloaded. The component removes the other
  URL candidates from the list. It keeps the `local()` candidates.
- The component downloads `<img>` sources and CSS `url()` images and inlines them as data URIs.
  PNG, JPEG and SVG images are accepted. GIF, WebP and AVIF images are not accepted.
- Relative URLs resolve against `<base href>`. Without a `<base href>`, relative URLs are
  removed.
- [lightningcss](https://crates.io/crates/lightningcss) parses each stylesheet and each `style`
  attribute. The component writes the CSS again in a short form. For example, `blue` becomes
  `#00f` and `bold` becomes `700`. The meaning of the CSS does not change.
- All downloads for one document must complete in 20 seconds. A resource that fails or is not
  readable is removed. The component writes a warning to stderr and makes the PDF.

The component embeds Liberation Sans and Liberation Serif. These fonts replace Arial, Helvetica,
Times New Roman and the generic `serif` and `sans-serif` families.

## Decisions

### A missing variable is an error

LiquidJS in generate-pdf 2.0 renders a variable or a property that does not exist as empty
text. This component does not. A blank space where you expect a value hides a mistake in the
template or in the data. An error shows the mistake at once.

This decision also keeps the code small. A lenient mode needs a custom Liquid runtime and a copy
of the Liquid standard library registration, because the `liquid` crate has no lenient option.

### No download limits

The component does not limit the number of downloads or their size. Only the 20 second deadline
applies. A limit adds code and a failure mode that no template has caused yet. Add a limit when a
document shows the problem.

## Build and test

Build and test the component:

```sh
just build
just test
```

The end-to-end test is in `test/native_wasm_components/generate_pdf_test.exs`. It uses the built
`.wasm` file. Run it:

```sh
mix test test/native_wasm_components/generate_pdf_test.exs
```
