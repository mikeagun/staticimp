//! flexible template rendering
//!
//! # Description and usage
//!
//! rendertemplate provides very simple template expanding code (mainly with the [Render] trait)
//!
//! There are additional utils for tokenizing and expanding string placeholders (using `Render`)
//!
//! ## Main types/traits:
//!
//! [`Render<X,Y>`] - this is the main trait for something that renders `Y`s from `X`s
//!
//! [SimpleParser] - string tokenizer:
//! - very simple parser for {name} style placeholders
//! - acts as an iterator over slices of the original string (wrapped in [Token] to tag them)
//!
//!
//! - use simple parser ([SimpleParser]) to parse basic template strings
//!   - parses bracketed placeholders (`{placeholder}`) for rendering
//!   - iterates over [SimpleToken]s containing slices of input string
//!   - the only time copies are (maybe) made is when creating [SimpleToken::Rendered] tokens
//!     (and when collecting output tokens into a string)
//!
//! # The Code
//!
//! ## Generic Types and Lifetimes
//!
//! This module is very heavily templated, so I've tried to keep the generic type and lifetime
//! names consistent throughout to make maintaining/understanding the code easier
//!
//!
//! ### Traits with multiple generic types/lifetimes
//!
//! [Render] based generics:
//! - `X` - render input type (lifetime is `'x`)
//! - `Y` - render output type (lifetime is `'y`)
//! - `T` - [Render] type
//! - `It` - [Iterator] type
//!
//!
//!
//! **Features to implement**:
//! - proper documentation and examples
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
//! # Goals
//!
//! The main goals of this module are small, flexible, and extensible
//! - zero dependencies
//! - almost every interface uses generic types to support any arguments that implement the right traits
//!   - implementing the [Render] trait is usually all you need (there are also some default implementations)
//!   - not limited to rendering from/to strings, but any types
//!   - some useful traits/functions defined for Renders that take/return strings
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
//!
//!
//!
//! [Cow] : std::borrow::Cow
use std::borrow::Borrow;
use std::cmp::Eq;
use std::hash::Hash;
use std::ops::AddAssign;
use std::{borrow::Cow, collections::HashMap, ops::Deref};
//use std::fmt::Display;
use std::marker::PhantomData;

pub trait ExtendRef<T>
where
    Self: for<'a> AddAssign<&'a T>,
    T : ?Sized,
{
    fn extend_ref<It,F>(&mut self, iter: It, func: F)
    where
        It : IntoIterator,
        F: for<'b> Fn(&'b It::Item) -> &'b T,
    {
        for el in iter {
            self.add_assign(func(&el));
        }
    }
}
impl<T,U> ExtendRef<T> for U
where
    Self: for<'a> AddAssign<&'a T>,
    T : ?Sized,
{
}

/// helper trait for collecting iterators with a temporary reference
///
/// collects a `T` from an iterator over `V`s using a computed reference
///
/// lets you write `myiter.collect_ref(|x| x.get_some_reference())`
/// - solves lifetime issues of `myiter.map(|x| x.get_some_reference()).collect()`
///
pub trait CollectRef: IntoIterator
where
{
    fn collect_ref_into<T, V, F>(self, acc: T, func: F) -> T
    where
        T: ExtendRef<V>,
        V: ?Sized,
        F: for<'b> Fn(&'b Self::Item) -> &'b V;
    fn collect_ref<T, V, F>(self, func: F) -> T
    where
        T: Default + ExtendRef<V>,
        V: ?Sized,
        F: for<'b> Fn(&'b Self::Item) -> &'b V;
}

/// CollectRef implemented for all iterators
impl<It> CollectRef for It
where
    It: Iterator,
{
    /// Collect T using [AddAssign] over items in iterator
    ///
    /// Arguments:
    /// - `func` - function to map item reference to reference used for collecting
    fn collect_ref_into<T, V, F>(self, mut acc: T, func: F) -> T
    where
        T: ExtendRef<V>,
        V: ?Sized,
        F: for<'b> Fn(&'b Self::Item) -> &'b V,
    {
        for el in self {
            acc += func(&el)
        }
        acc
    }
    /// Collect T using `collect_ref_into` (starting from `T::default()`)
    ///
    /// Arguments:
    /// - `func` - function to map item reference to reference used for collecting
    fn collect_ref<T, V, F>(self, func: F) -> T
    where
        T: Default + ExtendRef<V>,
        V: ?Sized,
        F: for<'b> Fn(&'b Self::Item) -> &'b V,
    {
        self.collect_ref_into(T::default(),func)
    }
}

///// rendertemplate module error
//pub enum Error {
//    BadParse(&'static str),
//}

//TODO: implement traits/functions returning Result (also see Error above)
///// rendertemplate [Result]
//type Result<T> = core::result::Result<T,Error>;

/// generic trait for something that renders
pub trait Render<X: ?Sized, Y: ?Sized>
{
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

/// generic trait for something renders and is consumed in the rendering
///
/// Useful for chaining intermediate renders (e.g. modifications over iterators)
pub trait RenderIterator<X, Y>: Iterator {
    /// Render Y from X, consuming self
    fn render_iter(self, x: X) -> Y;
}

/// simple parser tokens (which are slices/owned strings)
///
/// Except for `Rendered` (which is String), all variants are just string slices
///
/// This is used by [SimpleParser] to tag token slices of the input string.
///
/// - `Literal` - raw/literal text to return
/// - `Placeholder` - placeholder needing replacement (without braces)
/// - `Rendered` - rendered text [String] -- allows owned strings to be returned
/// - `Unterminated` - unterminated placeholder at end of string
#[derive(Debug, PartialEq, Eq)]
pub enum SimpleToken<'a> {
    /// Token of raw text
    Literal(&'a str),

    /// Placeholder to expand
    Placeholder(&'a str),

    /// Rendered text (Owned string)
    Rendered(String),

    /// unterminated placeholder/escape
    ///
    /// allows for chunked parsing, and will always be the last token returned before None
    Unterminated(&'a str),
    //Escape(&'a str),
}

/// generic trait text tokens which may be placeholders
pub trait RenderToken<'a> : From<&'a str> + From<String> {
    /// whether this token is a placeholder (i.e. to be expanded)
    fn is_placeholder(&self) -> bool;

    /// token text (including non-display text like placeholders)
    fn raw_ref(&self) -> &str;

    /// token text (ignoring text that shouldn't be displayed, e.g. placeholders)
    fn display_ref(&self) -> &str;
}

/// Utils for generic [RenderToken] iterators
pub trait TokenIterator<'x, Tok>: Iterator<Item = Tok> + Sized
where
    Tok: RenderToken<'x>,
{
    /// concatenate [RenderToken] display strings
    fn collect_display<T>(self) -> T
    where
        T: Default + for<'a> AddAssign<&'a str>,
    {
        //self.collect_ref(|tok| tok.display_ref())
        //self.collect_ref(|tok| tok.display_ref())
        let mut acc = T::default();

        acc.extend_ref(self, |tok| tok.display_ref());
        acc
    }

    /// concatenate [RenderToken] raw strings
    fn collect_raw<T>(self) -> T
    where
        T: Default + for<'a> AddAssign<&'a str>,
    {
        self.collect_ref(|tok| tok.raw_ref())
    }

    /// returns an iterator expands placeholders from self
    ///
    /// Calls T::render_placeholder on each item in self
    fn render_placeholders<'y, Y, T, YTok>(
        self,
        r: T,
    ) -> PlaceholderIt<'x, 'y, Y, T, Self, Tok, YTok>
    where
        Y: OptionalStr,
        YTok: RenderToken<'y> + From<Tok> + From<Y::Value>,
        T: RenderPlaceholder<Y>,
    {
        PlaceholderIt {
            it: self,
            r,
            _phantom: PhantomData,
        }
    }
}

/// Generic optional string
///
/// non-options are wrapped in `Some(self)`, and [Option]s just return `self`
/// - for `Option<T>` returns `self`
/// - else wraps value as `Some(self)`
///
/// This lets generic [Token] [Render]ing code handle both [Option]s and literal values
pub trait OptionalStr {
    type Value: AsRef<str> + Into<String>;
    /// get optional string value
    /// - for `Option<T>` returns `self`
    /// - else wraps value as `Some(self)`
    fn value(self) -> Option<Self::Value>;
}

/// helper trait to render [RenderToken]s by replacing Placeholders
///
/// wraps render that takes a string and returns a string/option into one that
/// takes+returns a RenderToken
/// - replaces placeholders using `self.render()`, and returns non-placeholders as-is
///
/// ignores non [Placeholder](Token::Placeholder)s
pub trait RenderPlaceholder<Y>: for<'a> Render<&'a str, Y>
where
    Y: OptionalStr,
{
    /// if tok is a Placeholder, render it (otherwise return tok)
    ///
    /// render result is treated as OptionalStr
    /// - if value is None returns tok
    /// - if value is Some(s), returns owned [Token::Rendered] with s
    fn render_placeholder<'x, 'y, XTok, YTok>(&self, tok: XTok) -> YTok
    where
        XTok: RenderToken<'x>,
        YTok: RenderToken<'y> + From<XTok> + From<Y::Value>,
    {
        if tok.is_placeholder() {
            self.render(tok.raw_ref())
                .value()
                .and_then(|r| Some(YTok::from(r)))
                .unwrap_or_else(move || tok.into())
        } else {
            tok.into()
        }
    }
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
pub struct RenderIt<Y, T, It>
where
    T: Render<It::Item, Y>,
    It: Iterator,
{
    /// iterator we are rendering from
    it: It,
    /// renderer to use on each `Item`
    r: T,
    /// placeholder so we can have Y in type constraints
    _phantom: PhantomData<Y>,
}

/// token rendering iterator
///
/// Calls `r.render(i)` on each placeholder in `it`
/// - non-placeholders are passed through
///
pub struct PlaceholderIt<'x, 'y, Y, T, It, XTok,YTok = XTok>
where
    Y: OptionalStr,
    T: RenderPlaceholder<Y>,
    It: TokenIterator<'x, XTok>,
    XTok: RenderToken<'x>,
    YTok: RenderToken<'y> + From<XTok> + From<Y::Value>,
{
    /// token iterator we are rendering from
    it: It,
    /// renderer to use
    r: T,
    /// placeholder for generic types needed in impl
    _phantom: PhantomData<(Y, &'x XTok, &'y YTok)>,
}

/// Something that can be parsed by [SimpleParser]
pub trait SimpleParse : AsRef<str> {
    /// creates parser for string
    fn parse_simple(&'_ self) -> SimpleParser<'_> {
        SimpleParser::new(self.as_ref())
    }
}

/// Implement `Render<X, Y>` for any `Fn(X) -> Y`
impl<F, X, Y> Render<X, Y> for F
where
    F: Fn(X) -> Y,
{
    fn render(&self, x: X) -> Y {
        self(x)
    }
}

////TODO: sort out closure support for RenderPlaceholder
//// - if RenderPlaceholder takes lifetime closure support is easy, but I don't have owned return
//// values figured out yet without for<'x>
//// - maybe something like Deserialize<'a> + DeserializeOwned
///// Implement `Render<X, Y>` for any `Fn(X) -> Y`
//impl<'x, F, X, Y> Render<&'x X, Y> for F
//where
//    F: for<'x> Fn(&'x X) -> Y,
//{
//    fn render(&self, x: X) -> Y {
//        self(x)
//    }
//}

/// Implement `RenderTo<X, Y>` for any `FnOnce(X) -> Y`
impl<F, X, Y> RenderTo<X, Y> for F
where
    F: FnOnce(X) -> Y,
{
    fn render_to(self, x: X) -> Y {
        self(x)
    }
}

impl<F, X, Y> RenderMut<X, Y> for F
where
    F: FnMut(X) -> Y,
{
    fn render_mut(&mut self, x: X) -> Y {
        self(x)
    }
}

/// lets string-likes be parsed by [SimpleParser]
impl<T> SimpleParse for T
where
    T: AsRef<str>,
{
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

impl<'a> RenderToken<'a> for SimpleToken<'a>
{
    fn is_placeholder(&self) -> bool {
        match self {
            SimpleToken::Placeholder(_) => true,
            _ => false,
        }
    }

    fn raw_ref(&self) -> &str {
        //&self
        use SimpleToken::*;
        match self {
            Literal(s) | Placeholder(s) | Unterminated(s) => &s,
            Rendered(s) => s.as_ref(),
        }
    }

    fn display_ref(&self) -> &str {
        use SimpleToken::*;
        match self {
            Literal(s) => &s,
            Rendered(s) => s.as_ref(),
            Placeholder(_) | Unterminated(_) => &"",
        }
    }
}

impl<'a> From<String> for SimpleToken<'a> {
    fn from(value: String) -> Self {
        Self::Rendered(value)
    }
}

impl<'a> From<Cow<'a, str>> for SimpleToken<'a> {
    fn from(value: Cow<'a, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Self::Literal(s),
            Cow::Owned(s) => Self::Rendered(s),
        }
    }
}


impl<'a> From<&'a str> for SimpleToken<'a> {
    fn from(value: &'a str) -> Self {
        Self::Literal(value)
    }
}

/// Deref SimpleToken to contained String/slice
impl Deref for SimpleToken<'_> {
    /// [Token]s deref to &str
    type Target = str;

    /// Dereference contained slice
    ///
    /// - Empty returns ""
    /// - other variants return contained/referenced slice
    fn deref(&self) -> &str {
        use SimpleToken::*;
        match self {
            Literal(s) | Placeholder(s) | Unterminated(s) => &s,
            Rendered(s) => s.as_ref(),
        }
    }
}

/// `Option<T>` optional string (returns `self`)
impl<T> OptionalStr for Option<T>
where
    T: OptionalStr,
{
    /// Inner type of Option
    type Value = T::Value;
    /// get self
    fn value(self) -> Option<T::Value> {
        self.and_then(|val| val.value())
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
    T: ?Sized + ToOwned,
    Cow<'a, T>: AsRef<str> + Into<String>,
{
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// `&Cow<T>` optional string (returns `Some(self)`)
impl<'a, T> OptionalStr for &'a Cow<'a, T>
where
    T: ?Sized + ToOwned,
    &'a Cow<'a, T>: AsRef<str> + Into<String>,
{
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// generic placeholder rendering
///
/// wraps any renderer that takes &str and returns [OptionalStr] to render Tokens by replacing
/// placeholders (other tokens returned unmodified)
impl<Y, T> RenderPlaceholder<Y> for T
where
    Y: OptionalStr,
    T: for<'x> Render<&'x str, Y>,
{
}

/// generic render implementation for iterators
///
/// wraps [Iterator] in [RenderIt] (which iterates over the render of each item in It)
impl<Y, T, It> RenderIterator<T, RenderIt<Y, T, It>> for It
where
    T: Render<It::Item, Y>,
    It: Iterator,
{
    /// render each item using r
    ///
    /// returns a RenderIt wrapped around self (which calls T::render on each Item in self)
    fn render_iter(self, r: T) -> RenderIt<Y, T, It> {
        RenderIt {
            it: self,
            r,
            _phantom: PhantomData,
        }
    }
}

/// RenderIt iterator implementation
///
/// calls render on each Item in wrapped iterator
impl<Y, T, It> Iterator for RenderIt<Y, T, It>
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
        self.it.next()
            .map(|i| self.r.render(i))
    }
}

/// impl TokenIterator for any [Iterator] over [Token]s
impl<'x, T, Tok> TokenIterator<'x, Tok> for T
where
    T: Iterator<Item = Tok>,
    Tok: RenderToken<'x>,
{
}

/// TokenIt iterator implementation
///
/// calls render on Placeholder, with other tokens passing through
impl<'x, 'y, Y, T, It, XTok, YTok> Iterator for PlaceholderIt<'x, 'y, Y, T, It, XTok, YTok>
where
    Y: OptionalStr,
    T: RenderPlaceholder<Y>,
    It: TokenIterator<'x, XTok>,
    XTok: RenderToken<'x>,
    YTok: RenderToken<'y> + From<XTok> + From<Y::Value>,
{
    /// iterates over [Token]s
    type Item = YTok;
    /// return/render next item (rendering item if its a [Placeholder](Token::Placeholder)
    fn next(&mut self) -> Option<YTok> {
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
impl<'y, X, K, Y> Render<&X, Option<&'y Y>> for &'y HashMap<K, Y>
//impl<X, K, Y> Render<&X, Option<&Y>> for HashMap<K, Y>
where
    X: ?Sized + Eq + Hash,
    K: Eq + Hash + Borrow<X>,
{
    /// render hashmap key to hashmap value
    /// - returns [Option] from [HashMap::get]
    fn render(&self, x: &X) -> Option<&'y Y> {
        //fn render(&self, x: &X) -> Option<&Y> {
        self.get(x)
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
    type Item = SimpleToken<'a>;

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

        use SimpleToken::*;
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

/// render a string slice by parsing and rendering placeholders
///
/// - parses with SimpleParser
/// - drops unresolved and unterminated placeholders
///
/// Type parameters:
/// - `Y` - output type from `T::render()` (implements [OptionalStr])
/// - `T` - [Render] that implements [`RenderPlaceholder`] and returns `Y`
/// - `Z` - output type to be collected from tokens
///
/// Arguments:
/// - `text` - string to parse and render
/// - `render` - [RenderPlaceholder] called on each placeholder token
pub fn render_str<'x, Y, T, Z>(text: &'x str, render: T) -> Z
where
    Y: 'x + OptionalStr,
    T: RenderPlaceholder<Y>,
    Z: Default + for<'b> AddAssign<&'b str>,
    SimpleToken<'x>: From<Y::Value>,
{
    SimpleParser::new(text)
        .render_placeholders::<_, _, SimpleToken>(render)
        .collect_display()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// test [SimpleParser] parsing
    fn test_parse() {
        let mut tokens = "Hello World!".parse_simple();
        assert_eq!(tokens.next(), Some(SimpleToken::Literal("Hello World!")));
        assert_eq!(tokens.next(), None);

        let mut tokens = "{a}".parse_simple();
        assert_eq!(tokens.next(), Some(SimpleToken::Placeholder("a")));
        assert_eq!(tokens.next(), None);

        let mut tokens = "Hello {name}!".parse_simple();
        assert_eq!(tokens.next(), Some(SimpleToken::Literal("Hello ")));
        assert_eq!(tokens.next(), Some(SimpleToken::Placeholder("name")));
        assert_eq!(tokens.next(), Some(SimpleToken::Literal("!")));
        assert_eq!(tokens.next(), None);

        let mut tokens = "{a}{b}".parse_simple();
        assert_eq!(tokens.next(), Some(SimpleToken::Placeholder("a")));
        assert_eq!(tokens.next(), Some(SimpleToken::Placeholder("b")));
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
    fn test_hashmap_placeholder_render() {
        let template = "Hello {name}!";
        let context: HashMap<_, _> = [("name", "World")].into_iter().collect();

        assert_eq!((&context).render(&"name"), Some(&"World"));

        let rendered: String = render_str(&template, &context);
        assert_eq!(&rendered, "Hello World!");

        let context: HashMap<_, _> = [("name".to_string(), "World".to_string())]
            .into_iter()
            .collect();
        //assert_eq!((&context).render("name"), Some(&"World"));
        context.get("blah");
    }
    //TODO: sort out lifetimes and trait implementations so render_str can accept closures
    //#[test]
    ///// Test closure rendering
    //fn test_closure_placeholder_render() {
    //    let template = "Hello {name}!";
    //    let rendered: String = render_str(&template, |_| "");
    //    assert_eq!(rendered, "Hello World!");
    //}
}
