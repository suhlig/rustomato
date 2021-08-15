use clap::{crate_authors, crate_version, App, AppSettings};

fn main() {
  let matches = App::new("rustomato")
    .about("Simple Pomodoro timer")
    .version(crate_version!())
    .author(crate_authors!())
    .license("MIT")
    .setting(AppSettings::ColoredHelp)
    .subcommand(
      App::new("pomodoro")
        .about("Work with Pomodori")
        .subcommand(
          App::new("start")
            .about("starts a new Pomodoro")
        )
        .subcommand(App::new("finish").about("finishes an active Pomodoro")),
    )
    .subcommand(
      App::new("break")
        .about("Work with breaks")
        .subcommand(
          App::new("start")
            .about("starts a new break")
        )
        .subcommand(App::new("finish").about("finishes a break")),
    )
    .get_matches();

  match matches.subcommand() {
    Some(("pomodoro", pomodoro_matches)) => {
      match pomodoro_matches.subcommand() {
        Some(("start", _)) => {
          println!("Starting a new Pomodoro");
        }
        Some(("stop", _)) => {
          println!("Stopping the active Pomodoro");
        }
        _ => unreachable!(),
      }
    }
    None => println!("No subcommand was used"), // If no subcommand was used it'll match the tuple ("", None)
    _ => unreachable!(), // If all subcommands are defined above, anything else is unreachabe!()
  }
}
