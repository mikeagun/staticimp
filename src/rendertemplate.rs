//! very flexible template rendering
//!
//! The main goals of this module are small, flexible, and extensible
//! - zero dependencies
//! - almost every interface uses generic types to support any arguments that implement the right traits
//!   - implementing the [Render]/[RenderTo] trait is usually all you need (there are also some default implementations)
//!   - not limited to rendering from/to strings, but any types
//!   - some useful traits/functions defined for Renders that take/return strings
//! - includes simple parser ([SimpleParser]) to parse basic template strings
//!   - parses bracketed placeholders (`{placeholder}`) for rendering (currently no escaped or other features)
//!   - iterates over [Token]s containing slices of input string
//!   - the only time copies are (maybe) made is when creating [Cow::Owned] tokens from render outputs
//!     (and when collecting output tokens into a string)
//! - generic [Iterator]s let you apply [Render]s over Iterators, and chain renders together
//!   - they work with any [Render] input/output types, not just [Token] or string rendering
//!
//! Although performance was a secondary goal, the heavily-templated implementation gives the
//! compiler a lot of room to optimize, and the zero-copy implementation makes it very
//! memory-friendly
//!
//! This module is intended for cases where compiling a regex/parser isn't worth it, e.g. loading
//! configuration files containing many templates where a single template is only expanded once
//!
//! [SimpleParser] string tokenizer:
//! - very simple parser for {name} style placeholders
//! - acts as an iterator over slices of the original string (wrapped in [Token] to tage them)
//!
//!
//! # The Code
//!
//! ## Generic Types and Lifetimes
//!
//! This module is very heavily templated, so I've tried to keep the generic type and lifetime
//! names consistent throughout to make maintaining/understanding the code easier
//!
//! For traits only needing on type/lifetime, `T` and `'a` are used
//!
//! [Render] generics
//! - `X` - render input type (lifetime if needed is `'x`)
//! - `Y` - render output type (lifetime if needed is `'y`)
//! - `T` - [Render] type
//! - `It` - [Iterator] type (for functions/traits that work on iterators)
//!
//!
//!
//! **Features To Implement**:
//! - implement derive macro(s)
//! - implement support for Result/Error
//! - consider streaming iterators
//! - consider escapes for parser (Token::Escape)
//!
//!
//! # Examples
//!
//! Parse string placeholders
//! ```
//! use rendertemplate::SimpleParse;
//!
//! let mut tokens = "Hello {name}!".parse_simple();
//! assert_eq!(tokens.next(), Some(Token::Literal("Hello ")));
//! assert_eq!(tokens.next(), Some(Token::Placeholder("name")));
//! assert_eq!(tokens.next(), Some(Token::Literal("!")));
//! assert_eq!(tokens.next(), None);
//! ```
//!
//! render template string using HashMap for lookups
//! ```
//! use rendertemplate::render;
//!
//! let template = "Hello {name}!";
//! let context : HashMap<_,_> = [
//!     ("name","World")
//! ].into_iter().collect();
//! let rendered : String = render_str(&template, &context);
//! assert_eq!( &rendered, "Hello World!" );
//! ```
//!
//! render template string using custom [Render] implementation
//! ```
//! use rendertemplate::{Render,render};
//!
//! struct Context {
//!     name : &'static str
//! }
//!
//! impl Render<&str,Option<&'static str>> for Context {
//!     fn render(&self, arg : &str) -> Option<&'static str> {
//!         if arg == "name" {
//!             Some(self.name)
//!         } else {
//!             None
//!         }
//!     }
//! }
//!
//! let template = "Hello {name}!";
//! let context = Context { name : "World" };
//! let rendered : String = render_str(&template, context);
//!
//! assert_eq!( &rendered, "Hello World!" );
//!
//! ```
//!
//! [Cow] : std::borrow::Cow

use std::{borrow::Cow, collections::HashMap, ops::Deref};
//use std::fmt::Display;
use std::marker::PhantomData;

///// rendertemplate module error
//pub enum Error {
//    BadParse(&'static str),
//}

//TODO: implement traits/functions returning Result (also see Error above)
///// rendertemplate [Result]
//type Result<T> = core::result::Result<T,Error>;

/// generic trait for something that renders
///
/// This and [RenderTo] are the main trait that rendertemplate is built around
pub trait Render<X, Y> {
    /// Render Y from X
    fn render(&self, x: X) -> Y;
}

/// generic trait for something renders and is consumed in the rendering
///
/// Useful for chaining intermediate renders (e.g. modifications over iterators)
pub trait RenderTo<X, Y> {
    /// Render Y from X, consuming self
    fn render_to(self, x: X) -> Y;
}

/// generic trait for something that renders, and modifies itself in the rendering
pub trait RenderMut<X, Y> {
    /// Render Y from X, mutating self
    fn render_mut(&mut self, x: X) -> Y;
}

/// Text token
///
/// Except for `Rendered` (which is a [Cow]), all variants are just string slices
/// - Rendered lets you returned owned strings when e.g. template expansions are generated
///   dynamically
///
/// Produced by parser to tag token slices of the input string
///
/// - `Literal` - raw/literal text to return
/// - `Placeholder` - placeholder needing replacement (without braces)
/// - `Rendered` - rendered text [Cow] -- allows owned strings to be returned
/// - `Unterminated` - unterminated placeholder at end of string
#[derive(Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// Empty token
    Empty,

    /// Token of raw text
    Literal(&'a str),

    /// Placeholder to expand
    Placeholder(&'a str),

    /// Rendered text
    ///
    /// Uses Cow for more flexible rendering and to get around lifetime issues
    /// - e.g. can contain wrapped str or Cow of generated [String]
    Rendered(Cow<'a, str>),

    /// unterminated placeholder/escape
    ///
    /// allows for chunked parsing, and will always be the last token returned before None
    Unterminated(&'a str),
    //Escape(&'a str),
}

/// Utils for generic [Token] iterators
pub trait TokenIterator<'a>: Iterator<Item = Token<'a>> + Sized {
    /// concatenate [Token]s, dropping Placeholder/Unterminated tokens
    fn collect_display<T>(self) -> T
    where
        T: FromIterator<Token<'a>>,
    {
        self.filter(|tok| {
            //drop anything but Literal/Rendered tokens
            match tok {
                Token::Literal(_) | Token::Rendered(_) => true,
                _ => false,
            }
        })
        .collect()
    }
}

/// something that Derefs to &str
///
/// mainly to support [Token] renderers
/// - helper traits/functions that work on tokens take StringType
///
/// [OptionalStr] works with StringTypes
pub trait StringType: AsRef<str> {}

/// Generic optional string
///
/// non-options are wrapped in `Some(self)`, and [Option]s just return `self`
/// - for `Option<T>` returns `self`
/// - else wraps value as `Some(self)`
///
/// This lets generic [Token] [Render]ing code handle both [Option]s and literal values
pub trait OptionalStr {
    type Value: StringType;
    /// get optional string value
    /// - for `Option<T>` returns `self`
    /// - else wraps value as `Some(self)`
    fn value(self) -> Option<Self::Value>;
}

/// renders Token by replacing Placeholders
///
/// generic for any [`Render<&str,Y>`] where Y impl OptionalStr
///
/// ignores non [Placeholder](Token::Placeholder)s
pub trait RenderPlaceholder<'x, 'y, X, Y>: Render<X, Y>
where
    //'y : 'x,
    X: StringType,
    Y: OptionalStr,
    Y::Value: AsRef<str>,
{
    /// if tok is Placeholder returns render(tok) (or tok), else return tok
    ///
    /// wraps string handling code with token handling code (so you don't need to use tokens in
    /// your code)
    fn render_placeholder(&self, tok: Token<'x>) -> Token<'y>;
}

/// renders [Token] iterators by mapping Placeholders to their values
trait RenderPlaceholders<'x, 'y>: TokenIterator<'x> {
    /// creates iterator ([TokenIt]) over [Token]s in self which renders [Token::Placeholder]s
    /// using r
    fn render_placeholders<X, Y, T>(self, r: T) -> TokenIt<'x, 'y, X, Y, T, Self>
    where
        T: RenderPlaceholder<'x, 'y, X, Y>,
        X: StringType,
        Y: OptionalStr,
        Y::Value: AsRef<str>;
}

/// placeholder-parsing iterator
///
/// iterates over slices of the original string as [Token]s
///
/// you can use [`SimpleParse::parse_simple<S,T>()`] to construct a SimpleParser
///
/// Doesn't copy text, but keeps a slice of it (so lifetime is tied to text lifetime)
/// - returned tokens are also slices of text (zero clones made)
///
/// # Examples
/// ```
/// let tokens = "Hello {name}!".parse_simple();
/// assert_eq!(tokens.next, Some(Token::Literal("Hello ")));
/// assert_eq!(tokens.next, Some(Token::Placeholder("name")));
/// assert_eq!(tokens.next, Some(Token::Literal("!")));
/// assert_eq!(tokens.next, None);
/// ```
pub struct SimpleParser<'a> {
    /// text still to be parsed
    text: &'a str,
}

/// Rendering iterator
///
/// Calls `r.render(i)` on each item `i` in `it`
///
pub struct RenderIt<X, Y, T, It>
where
    T: Render<X, Y>,
    It: Iterator,
{
    /// iterator we are rendering from
    it: It,
    /// renderer to use on each `Item`
    r: T,
    /// placeholder so we can have X in type constraints
    _phantomx: PhantomData<X>,
    /// placeholder so we can have Y in type constraints
    _phantomy: PhantomData<Y>,
}

/// Token Rendering iterator
///
/// Calls `r.render(i)` on each Placeholder in `it`
/// - non-Placeholders are passed through
///
pub struct TokenIt<'x, 'y, X, Y, T, It>
where
    //'y : 'x,
    X: StringType,
    Y: OptionalStr,
    T: RenderPlaceholder<'x, 'y, X, Y>,
    It: TokenIterator<'x>,
{
    /// token iterator we are rendering from
    it: It,
    /// renderer to use
    r: T,
    /// lets us use X in type constraints
    _phantomx: PhantomData<&'x X>,
    /// lets us use Y in type constraints
    _phantomy: PhantomData<&'y Y>,
}

/// Something that can be parsed by [SimpleParser]
pub trait SimpleParse {
    /// constructs [SimpleParser] iterator to parse string
    fn parse_simple(&'_ self) -> SimpleParser<'_>;
}

/// lets string-likes be parsed by [SimpleParser]
impl<T> SimpleParse for T
where
    T: AsRef<str>,
{
    /// creates parser for string
    fn parse_simple(&'_ self) -> SimpleParser<'_> {
        SimpleParser::new(self.as_ref())
    }
}

///// Display for rendertemplate errors
//impl Display for Error {
//    /// write error to formatter
//    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//        match self {
//            Error::BadParse(msg) => write!(f,"Bad Parse: {}",msg)
//        }
//    }
//}

///// Display for rendertemplate tokens
/////
///// Renders [Literal] and [Rendered] as their contained text; other tokens are dropped
/////
///// [Display]: std::fmt::Display
//impl<'a> std::fmt::Display for Token<'a> {
//    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//        use Token::*;
//        match self {
//            Empty | Placeholder(_) | Unterminated(_) => Ok(()),
//            Literal(s) => write!(f,"{}",s),
//            Rendered(s) => write!(f,"{}",s),
//        }
//    }
//}

/// token creation helper functions
impl<'a> Token<'a> {
    /// Create rendered token
    ///
    /// returns owned [Cow]
    fn rendered<'b>(s: &'a str) -> Token<'b> {
        Token::Rendered(Cow::Owned(s.to_owned()))
    }
}

/// Since [Token]s are just [`Cow`]s/slices, they deref to slices
impl<'a> Deref for Token<'a> {
    /// [Token]s deref to &str
    type Target = str;

    /// Dereference contained slice
    ///
    /// - Empty returns ""
    /// - other variants return contained/referenced slice
    fn deref(&self) -> &str {
        use Token::*;
        match self {
            Empty => &"",
            Literal(s) | Placeholder(s) | Unterminated(s) => &s,
            Rendered(s) => s.as_ref(),
        }
    }
}

/// Collects String from [Token] iterator
impl<'a> FromIterator<Token<'a>> for String {
    /// build [String] by concatenating each token slice
    fn from_iter<T: IntoIterator<Item = Token<'a>>>(iter: T) -> Self {
        let mut buf = String::new();
        iter.into_iter().for_each(|x| buf.push_str(&x));
        buf
    }
}

/// Collects Cow from [Token] Iterator
impl<'a> FromIterator<Token<'a>> for Cow<'a, str> {
    /// build Owned [Cow] by concatenating each token slice
    fn from_iter<T: IntoIterator<Item = Token<'a>>>(iter: T) -> Self {
        let mut buf = String::new();
        iter.into_iter().for_each(|x| buf.push_str(&x));
        Cow::from(buf)
    }
}

/// a StringType is any string-like (derefs to str) that implements Clone
///
/// Clone is needed so Cow can convert to owned string
impl<T> StringType for T where T: AsRef<str> + Clone {}

/// `Option<T>` optional string (returns `self`)
impl<T> OptionalStr for Option<T>
where
    T: StringType,
{
    /// Inner type of Option
    type Value = T;
    /// get self
    fn value(self) -> Option<T> {
        self
    }
}

/// `&str` optional string (returns `Some(self`))
impl OptionalStr for &str {
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// `&&str` optional string (returns `Some(self)`)
impl<'a> OptionalStr for &&'a str {
    /// Type of string
    type Value = &'a str;
    /// get Some(self)
    fn value(self) -> Option<&'a str> {
        Some(self)
    }
}

/// `String` optional string (returns `Some(self)`)
impl OptionalStr for String {
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// `&String` optional string (returns `Some(self)`)
impl OptionalStr for &String {
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// `Cow<T>` optional string (returns `Some(self)`)
impl<'a, T> OptionalStr for Cow<'a, T>
where
    T: Clone + StringType,
    Cow<'a, T>: AsRef<str>,
{
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// `&Cow<T>` optional string (returns `Some(self)`)
impl<'a, T> OptionalStr for &Cow<'a, T>
where
    T: Clone + StringType,
    Cow<'a, T>: AsRef<str>,
{
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// generic Placeholder Token rendering
///
/// wraps any renderer that takes &str and returns [OptionalStr] to render Tokens by replacing
/// placeholders (other tokens returned unmodified)
impl<'a, Y, T> RenderPlaceholder<'a, 'a, &'a str, Y> for T
where
    Y: OptionalStr,
    T: Render<&'a str, Y>,
{
    /// if tok is a Placeholder, render it (otherwise return tok)
    ///
    /// render result is treated as OptionalStr
    /// - if value is None returns tok
    /// - if value is Some(s), returns owned [Token::Rendered] with s
    fn render_placeholder(&self, tok: Token<'a>) -> Token<'a> {
        if let Token::Placeholder(k) = tok {
            if let Some(val) = self.render(&k).value() {
                Token::rendered(val.as_ref())
            } else {
                tok
            }
        } else {
            tok
        }
    }
}

/// Token iterator rendering
///
/// implements `render_placeholders` to produce a new iterator over the items in self with
/// placeholders rendered
impl<'x, 'y, It> RenderPlaceholders<'x, 'y> for It
where
    It: TokenIterator<'x>,
{
    /// returns an iterator which renders placeholders from self
    ///
    /// Calls T::render_placeholder on each item in self
    fn render_placeholders<X, Y, T>(self, r: T) -> TokenIt<'x, 'y, X, Y, T, It>
    where
        X: StringType,
        Y: OptionalStr,
        T: RenderPlaceholder<'x, 'y, X, Y>,
    {
        TokenIt {
            it: self,
            r,
            _phantomx: PhantomData,
            _phantomy: PhantomData,
        }
    }
}

/// generic render implementation for iterators
///
/// wraps [Iterator] in [RenderIt] (which iterates over the render of each item in It)
impl<X, Y, T, It> RenderTo<T, RenderIt<X, Y, T, It>> for It
where
    T: Render<X, Y>,
    It: Iterator,
{
    /// render each item using r
    ///
    /// returns a RenderIt wrapped around self (which calls T::render on each Item in self)
    fn render_to(self, r: T) -> RenderIt<X, Y, T, It> {
        RenderIt {
            it: self,
            r,
            _phantomx: PhantomData,
            _phantomy: PhantomData,
        }
    }
}

/// RenderIt iterator implementation
///
/// calls render on each Item in wrapped iterator
impl<Y, T, It> Iterator for RenderIt<It::Item, Y, T, It>
where
    T: Render<It::Item, Y>,
    It: Iterator,
{
    type Item = Y;

    /// get render of next item
    ///
    /// - if next is None, returns None
    /// - else returns render of item
    fn next(&mut self) -> Option<Self::Item> {
        //if Some(i), return render of i, else return None
        self.it.next().map(|i| self.r.render(i))
    }
}

/// impl TokenIterator for any [Iterator] over [Token]s
impl<'a, T> TokenIterator<'a> for T where T: Iterator<Item = Token<'a>> {}

/// TokenIt iterator implementation
///
/// calls render on Placeholder, with other tokens passing through
impl<'x, 'y, X, Y, T, It> Iterator for TokenIt<'x, 'y, X, Y, T, It>
where
    //'y : 'x,
    X: StringType,
    Y: OptionalStr,
    T: RenderPlaceholder<'x, 'y, X, Y>,
    It: TokenIterator<'x>,
{
    /// iterates over [Token]s
    type Item = Token<'y>;
    /// return/render next item (rendering item if its a [Placeholder](Token::Placeholder)
    fn next(&mut self) -> Option<Token<'y>> {
        self.it
            .next()
            .map(move |tok| self.r.render_placeholder(tok))
    }
}

/// generic [Render] implementation for [HashMap]s
///
/// Implemented for HashMap reference so we can tie lifetimes together
///
/// returns result of HashMap lookup (option with value reference)
impl<'a, K, V> Render<K, Option<&'a V>> for &'a HashMap<&str, V>
where
    K: StringType + std::cmp::Eq + std::hash::Hash,
{
    /// render hashmap key to hashmap value
    /// - returns [Option] from [HashMap::get]
    fn render(&self, k: K) -> Option<&'a V> {
        self.get(k.as_ref())
    }
}

/// parsing helpers for SimpleParser
///
/// Chunking functions work on bytes, not chars
impl<'a> SimpleParser<'a> {
    /// Create a SimpleParser from a string slice
    ///
    /// Alternatively you can use [SimpleParse::parse_simple]
    /// - e.g. `"hello {name}".parse_simple()`
    ///
    /// - `text` - string to parse
    fn new(text: &'a str) -> Self {
        Self { text }
    }

    /// get next token slice
    ///  - `len` - byte length of chunk (or: byte index just after end of token)
    fn chunk(&mut self, len: usize) -> &'a str {
        let (ret, rest) = self.text.split_at(len);
        self.text = &rest;
        return ret;
    }

    /// get next token slice, skipping bytes before+after token
    ///
    ///  - `begin` - start of token (bytes up to there are dropped)
    ///  - `end` - byte index just after end of token
    ///  - `skip` - how many bytes to skip after splitting off chunk
    fn chunk_skip(&mut self, begin: usize, end: usize, skip_after: usize) -> &'a str {
        let (ret, rest) = self.text.split_at(end);
        let ret = &ret[begin..];
        self.text = &rest[skip_after..];
        return ret;
    }

    /// clear remainder string and return rest as one chunk
    fn rest(&mut self) -> &'a str {
        let ret = self.text;
        self.text = &"";
        return ret;
    }

    /// clear remainder string and return rest as one chunk (after skipping n bytes)
    ///
    /// - `n` - how many bytes to skip before returning rest
    fn rest_skip(&mut self, n: usize) -> &'a str {
        let ret = &self.text[n..];
        self.text = &"";
        return ret;
    }
}

/// SimpleParser [Token] iterator implementation
///
/// This is where the string actually gets parsed (in `next`)
impl<'a> Iterator for SimpleParser<'a> {
    /// SimpleParser iterates over [Token] slices from original text
    type Item = Token<'a>;

    /// get next Token slice
    ///
    /// - non-placeholder text returned as [Token::Literal]
    /// - placeholders returned as [Token::Placeholder] (after stripping braces)
    /// - if the text ends with an unterminated Placeholder, remainder returned as [Token::Unterminated]
    fn next(&mut self) -> Option<Self::Item> {
        //if there are >0 chars, first char determines token type, else we are done
        let c = match self.text.chars().next() {
            Some(c) => c,
            None => return None,
        };

        //
        //placeholders look like {placeholder}, so if c == '{' the next token is a placeholder
        // - else the next token (or the rest of the string) is a literal
        //

        use Token::*;
        Some(if c == '{' {
            //at start of placeholder token
            //look for end of placeholder
            match self.text.find('}') {
                // closing brace found. return Placeholder (without braces)
                Some(i) => Placeholder(self.chunk_skip(1, i, 1)),
                //Placeholder not terminated, so return Unterminated
                None => Unterminated(self.rest_skip(1)),
            }
        } else {
            //at start of literal token
            //look for start of placeholder
            match self.text.find('{') {
                //placeholder found, return the Literal up to it
                Some(i) => Literal(self.chunk(i)),
                //no placeholders found, so just return text as a Literal
                None => Literal(self.rest()),
            }
        })
    }
}

/// render a string slice with template replacement
///
/// - parses with SimpleParser
/// - drops unresolved and unterminated placeholders
///
/// Type parameters:
/// - `T` - input string type (something that derefs to &str)
/// - `U` - [Render] that implements [`RenderPlaceholder`] and returns `W`
/// - `V` - output type must implement [`FromIterator<Token>`]
/// - `W` - output from [Render] (must implement [`OptionalStr`])
///
/// Arguments:
/// - `text` - string to parse and render
/// - `render` - [RenderPlaceholder] called on each placeholder token
pub fn render_str<'x, 'y, X, Y, T, Z>(text: &'x str, render: T) -> Z
where
    X: 'x + StringType,
    Y: 'y + OptionalStr,
    T: RenderPlaceholder<'x, 'y, X, Y>,
    Z: FromIterator<Token<'y>>,
{
    SimpleParser::new(text)
        .render_placeholders(render)
        .collect_display()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// test [SimpleParser] parsing
    fn test_parse() {
        let mut tokens = "Hello World!".parse_simple();
        assert_eq!(tokens.next(), Some(Token::Literal("Hello World!")));
        assert_eq!(tokens.next(), None);

        let mut tokens = "{a}".parse_simple();
        assert_eq!(tokens.next(), Some(Token::Placeholder("a")));
        assert_eq!(tokens.next(), None);

        let mut tokens = "Hello {name}!".parse_simple();
        assert_eq!(tokens.next(), Some(Token::Literal("Hello ")));
        assert_eq!(tokens.next(), Some(Token::Placeholder("name")));
        assert_eq!(tokens.next(), Some(Token::Literal("!")));
        assert_eq!(tokens.next(), None);

        let mut tokens = "{a}{b}".parse_simple();
        assert_eq!(tokens.next(), Some(Token::Placeholder("a")));
        assert_eq!(tokens.next(), Some(Token::Placeholder("b")));
        assert_eq!(tokens.next(), None);
    }

    #[test]
    /// test [render_str]
    fn test_render_str_impl() {
        struct Context {
            name: &'static str,
        }

        struct ContextRef {
            name: String,
        }

        impl Render<&str, Option<&'static str>> for Context {
            fn render(&self, arg: &str) -> Option<&'static str> {
                if arg == "name" {
                    Some(self.name)
                } else {
                    None
                }
            }
        }

        impl<'a> Render<&str, Option<&'a str>> for &'a ContextRef {
            fn render(&self, arg: &str) -> Option<&'a str> {
                if arg == "name" {
                    Some(&self.name)
                } else {
                    None
                }
            }
        }

        let template = "Hello {name}!";
        let context = Context { name: "World" };
        let rendered: String = render_str(&template, context);

        assert_eq!(&rendered, "Hello World!");

        let context = ContextRef {
            name: "World".to_string(),
        };
        let rendered: String = render_str(&template, &context);

        assert_eq!(&rendered, "Hello World!");
    }

    #[test]
    /// Test default [HashMap] render impl
    fn test_hashmap_placeholer_render() {
        let template = "Hello {name}!";
        let context: HashMap<_, _> = [("name", "World")].into_iter().collect();

        assert_eq!((&context).render("name"), Some(&"World"));

        let rendered: String = render_str(&template, &context);
        assert_eq!(&rendered, "Hello World!");
    }
}
