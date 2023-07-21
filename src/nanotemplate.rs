//! minimal and very flexible template rendering
//!
//! The main goals are small, flexible, and extensible
//!
//! zero dependencies
//!
//!
//! - almost every interface is completely generic to support any arguments that implement the right traits
//!   - implementing the [Render] trait is usually all you need (there are also some default implementations)
//!   - not limited to rendering from/to strings, but any types
//! - includes simple parser ([SimpleParser]) (currently very basic) to iterate over [Token] slices of an input string
//!   - parses bracketed placeholders (`{placeholder}`) for rendering (currently no escaped or other features)
//!   - zero copies made in parser, just slices
//!   - the only time copies are (maybe) made is when creating [Cow::Owned] tokens from render outputs
//!     (and when collecting output tokens into a string)
//! - generic [Iterator]s let you apply [Render]s over Iterators, and chain renders together
//!   - they work with any [Render] input/output types, not just [Token] or string rendering
//!
//! Although performance was a secondary goal, the heavily-templated implementation making good use
//! of chained iterators give the compiler a lot of room to optimize,
//! and the zero-copy implementation makes it very memory-friendly
//!
//! very simple expander for {name} style placeholders
//!   - doesn't currently support escapes
//!   - replaces missing fields with `""`
//!
//! Intended for cases where compiling a regex/parser isn't worth it,
//! e.g. loading configuration files containing templates where a single template is only
//!   expanded once
//!
//! The module itself is extremely simple, most of the code complexity comes from generic
//! lifetime+type management.
//!
//! [Cow] : std::borrow::Cow
//!
//! # Examples
//!
//! Parse string placeholders
//! ```
//! use nanotemplate::SimpleParse;
//!
//! let tokens = "Hello {name}!".parse_simple();
//! assert_eq!(tokens.next, Some(Token::Literal("Hello ")));
//! assert_eq!(tokens.next, Some(Token::Placeholder("name")));
//! assert_eq!(tokens.next, Some(Token::Literal("!")));
//! assert_eq!(tokens.next, None);
//! ```
//!
//! render template string using HashMap for lookups
//! ```
//! use nanotemplate::render;
//!
//! let template = "Hello {name}!";
//! let context : HashMap<'_,'_> = [
//!     ("name","World")
//! ].into_iter().collect();
//! assert_eq!( render(template, context), "Hello World!".to_string() );
//! ```
//!
//! render template string using custom [Render] implementation
//! ```
//! use nanotemplate::{Render,render};
//!
//! struct Context {
//!     name;
//! }
//!
//! impl Render<&str> for Context {
//!     type Target = Option<&'static str>;
//!     fn render(&self, arg : &str) -> Self::Target {
//!         if (arg == "name") {
//!             Some(self.name)
//!         } else {
//!             None
//!         }
//!     }
//! }
//!
//! let context = Context { name : "World" };
//! 
//! assert_eq!( render(template, context), "Hello World!".to_string() );
//!
//! ```

use std::{borrow::Cow, collections::HashMap, ops::Deref, marker::PhantomData};
//use std::fmt::Display;

//TODO: support nanotemplate::Error (see note below on Result)
///// nanotemplate module error
//pub enum Error {
//    BadParse(&'static str),
//}

//TODO: implement traits/functions returning Result
///// nanotemplate [Result]
//type Result<T> = core::result::Result<T,Error>;

/// Text tokens
///
/// Except for `Placeholder` (which is a [Cow]), all variants are just string slices
///
/// - `Literal` - raw text to return
/// - `Placeholder` - placeholder needing replacement
/// - `Rendered` - rendered text [Cow]
/// - `Unterminated` - unterminated expansion at end of slice
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
    Rendered(Cow<'a,str>),

    /// unterminated placeholder/escape
    ///
    /// allows for chunked parsing, and will always be the last token returned before None
    Unterminated(&'a str),

    //Escape(&'a str),
}

/// generic trait for something that can produce `Target`s from `T`s
pub trait Render<T> {
    /// type that &Self renders to
    type Target;
    /// Render a Target from a T
    fn render(&self, arg : T) -> Self::Target;
}

/// render trait for something that gets consumed in the rendering
///
/// Useful for chaining intermediate renders (e.g. iterator)
pub trait RenderTo<T> {
    /// type that Self renders to
    type Target;
    /// Render a Target from a T
    fn render_to(self, arg : T) -> Self::Target;
}

/// something that Derefs to &str 
///
/// mainly to support [Token] renders
///
/// [OptionalStr] works with StringTypes
pub trait StringType<'a> : AsRef<str> {
}

/// Generic optional string
///
/// non-options are wrapped in `Some(self)`, and [Option]s just return `self`
/// - for Option<T> returns self
/// - else wraps value as `Some(self)`
///
/// This lets generic [Token] [Render]ing code handle both [Option]s and literal values
/// - also gives the compiler an opportunity to optimize for the option/value cases that are actually used
pub trait OptionalStr<'a> {
    type Value : StringType<'a>;
    /// get optional string value
    /// - for Option<T> returns self
    /// - else wraps value as `Some(self)`
    fn value(self) -> Option<Self::Value>;
}

/// renders Token by replacing Placeholders
///
/// automatically implemented for any [Render]<&str,V> where V is string-like (derefs to str)
pub trait RenderPlaceholder<'a,T> : Render<&'a str, Target = T> where T : OptionalStr<'a>, T::Value : AsRef<str> {
    /// if tok is Placeholder returns render(tok), else return tok
    fn render_placeholder(&self, tok : Token<'a>) -> Token<'a>;
}

/// renders [Token] iterators by mapping Placeholders to their values
trait RenderPlaceholders<'a,T,U> : TokenIterator<'a> where T : RenderPlaceholder<'a,U>, U : OptionalStr<'a>, U::Value : AsRef<str> {
    /// wraps [Token] iterator in [TokenIt] so we can render [Token::Placeholder]s
    fn render_placeholders(self,r : T) -> TokenIt<'a,T,U,Self>;
}

/// placeholder-parsing iterator
///
/// iterates over slices of the original string as [Token]s
///
/// you can use [`SimpleParse::parse_simple`]<S,T>(s,t) to construct a SimpleParser
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
    text : &'a str,
}

/// Rendering iterator
///
/// Calls `r.render(i)` on each item `i` in `it`
///
pub struct RenderIt<I,T : Render<I>,It : Iterator<Item = I>> {
    /// iterator we are rendering from
    it : It,
    /// renderer to use on each `Item`
    r : T
}

/// Generic iterator over [Token]s
pub trait TokenIterator<'a> : Iterator<Item = Token<'a>>+Sized {
    /// concatenate [Token]s, dropping Placeholder/Unterminated
    fn collect_display<'b,T : 'b+FromIterator<Token<'a>>>(self) -> T {
        self.filter(|tok| {
            //drop anything but Literal/Rendered tokens
            match tok {
                Token::Literal(_) | Token::Rendered(_) => true,
                _ => false
            }
        }).collect()
    }
}

/// Token Rendering iterator
///
/// Calls `r.render(i)` on each Placeholder in `it`
/// - non-Placeholders are passed through
///
pub struct TokenIt<'a,T,U,It> where T : RenderPlaceholder<'a,U>, U : OptionalStr<'a>, U::Value : AsRef<str>, It : TokenIterator<'a> {
    /// token iterator we are rendering from
    it : It,
    /// renderer to use
    r : T,
    /// just to prevent errors on `U` being unused
    /// - `'a` and `U` needed to tie the types together
    _phantom : PhantomData<&'a U>
}

/// Something that can be parsed by [SimpleParser]
pub trait SimpleParse {
    /// constructs [SimpleParser] iterator to parse string
    fn parse_simple(&self) -> SimpleParser;
}

/// lets string-likes be parsed by [SimpleParser]
impl<S> SimpleParse for S where S : AsRef<str> {
    /// creates parser for string
    fn parse_simple(&self) -> SimpleParser {
        SimpleParser::new(self.as_ref())
    }
}

///// Display for nanotemplate errors
//impl Display for Error {
//    /// write error to formatter
//    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//        match self {
//            Error::BadParse(msg) => write!(f,"Bad Parse: {}",msg)
//        }
//    }
//}

///// Display for nanotemplate tokens
/////
///// Renders [Literal] and [Rendered] as their contained text; other tokens are dropped
/////
///// [Display]: std::fmt::Display
//impl<'a> std::fmt::Display for Token<'a> {
//    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//        use Token::*;
//        match self {
//            Empty => Ok(()),
//            Literal(s) => write!(f,"{}",s),
//            Placeholder(_) => Ok(()),
//            Rendered(s) => write!(f,"{}",s),
//            Unterminated(s) => Ok(())
//        }
//    }
//}

/// token creation helper functions
impl<'a> Token<'a> {
    /// Create rendered token
    ///
    /// returns owned [Cow]
    fn rendered(s : &'a str) -> Token<'static> {
        Token::Rendered(Cow::Owned(s.to_owned()))
    }
}

/// Since [Token]s are just [`Cow`]s/slices, they can deref to slices
impl<'a> Deref for Token<'a> {
    /// [Token]s deref to &str
    type Target = str;

    /// Dereference contained slice
    ///
    /// - Empty returns ""
    /// - slice variants return &value (Literal,Placeholder,Unterminated)
    /// - Cow variants return value.as_ref()
    fn deref(&self) -> &str {
        use Token::*;
        match self {
            Empty => &"",
            Literal(s) | Placeholder(s) | Unterminated(s) => &s,
            Rendered(s) => s.as_ref()
        }
    }
}

/// Lets strings be collected from [Token] iterators
impl<'a> FromIterator<Token<'a>> for String {
    /// build [String] by concatenating each token
    ///
    /// takes deref of token to get contained slice
    fn from_iter<T: IntoIterator<Item = Token<'a>>>(iter: T) -> Self {
        let mut buf = String::new();
        iter.into_iter().for_each(|x| buf.push_str(&x));
        buf
    }
}

/// a StringType is any string-like (derefs to str) that implements Clone
impl<'a,T> StringType<'a> for T where T : AsRef<str>+Clone {}

/// Option<T> optional string (returns self)
impl<'a,T> OptionalStr<'a> for Option<T> where T : StringType<'a> {
    /// Inner type of Option
    type Value = T;
    /// get self
    fn value(self) -> Option<T> {
        self
    }
}

/// &str optional string (returns Some(self))
impl<'a> OptionalStr<'a> for &str {
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// String optional string (returns Some(self))
impl<'a> OptionalStr<'a> for String {
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// Cow<T> optional string (returns Some(self))
impl<'a,T : Clone+StringType<'a>> OptionalStr<'a> for Cow<'a,T> where Cow<'a,T> : AsRef<str> {
    /// Type of string
    type Value = Self;
    /// get Some(self)
    fn value(self) -> Option<Self> {
        Some(self)
    }
}

/// generic Token rendering
///
/// lets any renderer that takes &str and returns T : [OptionalStr] render Tokens by replacing
/// placeholders (other tokens returned unmodified)
impl<'a,T,U> RenderPlaceholder<'a,T> for U where U : Render<&'a str,Target = T>, T : OptionalStr<'a>, T::Value : AsRef<str> {
    /// if tok is a Placeholder, render it (otherwise return tok)
    /// - render result treated as OptionalStr, if value is None also returns tok
    fn render_placeholder(&self, tok : Token<'a>) -> Token<'a> {
        if let Token::Placeholder(k) = tok {
            if let Some(val) = self.render(k.as_ref()).value() {
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
impl<'a,T,U,It> RenderPlaceholders<'a,T,U> for It where
    It : TokenIterator<'a>,
    T : RenderPlaceholder<'a,U>,
    U : OptionalStr<'a>,
    U::Value : AsRef<str> {
    /// returns an iterator which renders placeholders from self
    fn render_placeholders(self,r : T) -> TokenIt<'a,T,U,Self> {
        TokenIt { it : self, r, _phantom : PhantomData }
    }
}

/// generic render implementation for iterators
///
/// wraps [Iterator] in [RenderIt]
impl<I,T,It> RenderTo<T> for It where T : Render<I>, It : Iterator<Item = I> {
    /// returns a RenderIt iterator wrapped around Self
    type Target = RenderIt<I,T,It>;

    /// render each item using r
    ///
    /// returns a RenderIt wrapped around self
    fn render_to(self, r : T) -> Self::Target {
        RenderIt { it : self, r }
    }
}

/// RenderIt iterator implementation
///
/// calls render on each item
impl<I,T,It> Iterator for RenderIt<I,T,It> where T : Render<I>, It : Iterator<Item = I> {
    /// returns whatever T : Render returns
    type Item = T::Target;

    /// get render of next item
    fn next(&mut self) -> Option<Self::Item> {
        self.it.next().map(|i| self.r.render(i))
    }
}

/// trait for [Iterator]s over [Token]s
impl<'a,T> TokenIterator<'a> for T where T : Iterator<Item = Token<'a>> { }

/// TokenIt iterator implementation
///
/// calls render on Placeholder, with other tokens passing through
impl<'a,T,U,It> Iterator for TokenIt<'a,T,U,It> where T : RenderPlaceholder<'a,U>, U : OptionalStr<'a>, U::Value : AsRef<str>, It : TokenIterator<'a> {
    /// iterates over [Token]s
    type Item = Token<'a>;
    /// get next item (rendering item if its a [Placeholder](Token::Placeholder)
    fn next(&mut self) -> Option<Self::Item> {
        self.it.next().map(|tok| self.r.render_placeholder(tok))
    }
}

/// generic [Render] implementation for [HashMap]s
///
/// returns result of HashMap lookup (option with value reference)
impl<'a,K,V> Render<&K> for &'a HashMap<K,V> where K : std::cmp::Eq+std::hash::Hash {
    type Target = Option<&'a V>;
    /// render hashmap key to hashmap value
    /// - returns [Option] from [HashMap::get]
    fn render(&self, k : &K) -> Option<&'a V> {
        self.get(&k)
    }
}


/// parsing helpers for SimpleParser
impl<'a> SimpleParser<'a> {
    /// Create a SimpleParser from a string slice
    ///
    /// Alternatively you can use [SimpleParse::parse_simple]
    /// - i.e. `stringlike.parse_simple()`
    ///
    /// - `text` - slice to parse
    fn new(text : &'a str) -> Self {
        Self {
            text
        }
    }

    /// get next token slice
    ///  - `i` - byte index just after end of token
    fn chunk(&mut self,i : usize) -> &'a str {
        let (ret,rest) = self.text.split_at(i);
        self.text = &rest;
        return ret;
    }

    /// get next token slice, skipping bytes before+after token
    ///
    ///  - `begin` - start of token (bytes up to there are dropped)
    ///  - `end` - byte index just after end of token
    ///  - `skip` - how many bytes to skip after splitting
    fn chunk_skip(&mut self,begin : usize, end : usize, skip_after : usize) -> &'a str {
        let (ret,rest) = self.text.split_at(end);
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
    fn rest_skip(&mut self, n : usize) -> &'a str {
        let ret = &self.text[n..];
        self.text = &"";
        return ret;
    }
}

/// SimpleParser iterator
impl<'a> Iterator for SimpleParser<'a> {
    /// SimpleParser iterates over [Token] slices from original text
    type Item = Token<'a>;

    /// get next token
    ///
    /// - non-placeholder text returned as [Token::Literal]
    /// - placeholders returned as [Token::Placeholder] (after stripping braces)
    /// - if the text ends with an unterminated Placeholder, remainder returned as [Token::Unterminated]
    fn next(&mut self) -> Option<Self::Item> {
        use Token::*;

        let mut it = self.text.char_indices();
        let first = it.next();
        //if empty we are done (so return None)
        if first == None {
            return None
        }
        //else we got at least one char, we use it to determine the token type
        let (_,c) = first.unwrap();

        //
        //placeholders look like {placeholder}, so if c == '{' the next token is a placeholder
        // - else the next token (or the rest of the string) is a literal
        //

        Some(if c == '{' {
            //If placeholder is closed, return it (without the braces)
            if let Some((i,_)) = it.find(|(_,c)| *c == '}') {
                // closing brace found. return Placeholder
                Placeholder(self.chunk_skip(1,i,1))
            } else {
                //Placeholder not terminated, so return Unterminated
                Unterminated(self.rest_skip(1))
            }
        } else {
            //If text contains placeholder, return the Literal up to it
            if let Some((i,_)) = it.find(|(_,c)| *c == '{') {
                //we return the Literal up to the next placeholder
                Literal(self.chunk(i))
            } else {
                //no placeholders found, so just return text as a Literal
                Literal(self.rest())
            }
        })
    }
}

/// render a string slice with template replacement
///
/// - parses with SimpleParser
/// - drops unresolved and unterminated placeholders
pub fn render<'a,T,U,V,W>(text : &'a T, render : U) -> V where
    T : AsRef<str>,
    U : RenderPlaceholder<'a,W>,
    V : FromIterator<Token<'a>>,
    W : 'a+OptionalStr<'a>, {
    text.parse_simple()
        .render_placeholders(render)
        .collect_display()
}
