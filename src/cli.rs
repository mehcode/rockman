use clap::{App, AppSettings, Arg, SubCommand};

fn search() -> App<'static, 'static> {
    SubCommand::with_name("search")
        .long_about("Search for packages")
        .setting(AppSettings::ColoredHelp)
        .arg(Arg::with_name("term").required(true).index(1))
}

fn info() -> App<'static, 'static> {
    SubCommand::with_name("info")
        .long_about("Show information for package")
        .setting(AppSettings::ColoredHelp)
        .arg(Arg::with_name("package").required(true).index(1).multiple(true))
}

fn download() -> App<'static, 'static> {
    SubCommand::with_name("download")
        .long_about("Download snapshot for package")
        .setting(AppSettings::ColoredHelp)
        .arg(Arg::with_name("package").required(true).index(1))
}

pub fn build() -> App<'static, 'static> {
    App::new(crate_name!())
        .version(crate_version!())
        .setting(AppSettings::ColoredHelp)
        .setting(AppSettings::DisableHelpSubcommand)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .setting(AppSettings::InferSubcommands)
        .setting(AppSettings::GlobalVersion)
        .subcommand(search())
        .subcommand(info())
        .subcommand(download())
}
