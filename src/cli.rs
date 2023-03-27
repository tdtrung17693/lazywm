use clap::Parser;

#[derive(Parser, Debug)] // requires `derive` feature
#[command(term_width = 0)] // Just to make testing across clap features easier
pub(crate) struct Args {
    #[arg(short = 'c', value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
    pub config: Option<String>,
}
