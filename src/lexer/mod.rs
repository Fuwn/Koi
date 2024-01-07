pub use peek::PeekableLexer as Lexer;
use {raw::RawLexer, record::RecordingLexer};

mod peek;
mod raw;
mod record;

#[cfg(test)] mod test;

pub fn new(source: String) -> Lexer {
  Lexer::new(RecordingLexer::new(RawLexer::new(source)))
}
