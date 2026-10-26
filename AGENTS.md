# Coding standards

These rules apply to every file in this repository.

## No comments in code

Do not write comments in Rust, WIT, Elixir or Just files. This includes doc comments (`///`,
`//!`) and inline comments (`//`, `#`). Put the explanation in the name of the function, the
type or the test. Put the behaviour that a reader must know in the README of the component.

Attributes are not comments. Keep an attribute only when the build needs it.

## Write documentation in Simplified Technical English

Write every README, commit message and pull request description in ASD-STE100 Simplified
Technical English (STE):

- Write one topic in each sentence.
- Write a maximum of 20 words in an instruction and a maximum of 25 words in a description.
- Write a maximum of six sentences in a paragraph.
- Use the active voice. Use the present tense.
- Write an instruction as a command: "Build the component", not "The component should be built".
- Use one word for one meaning. Use the same word for the same thing in the whole document.
- Use the articles "the", "a" and "an". Use "this" and "these" to point at a thing.
- Do not use idioms, slang or figures of speech.
- Do not join more than three nouns. Write "the limit for the size of the response", not "the
  response size limit".
- Use a list for steps and for a set of parallel items.
- Use a word that ends in "-ing" only when it is part of a technical name.
