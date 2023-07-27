use colored::ColoredString;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::sync::Arc;
use console::Term;
use crate::cli_core::{CLICore, Command, CommandBuilder, InputError};
use crate::Server;

// FIXME: maybe add tab completion

pub struct CommandLineInterface {
    core: CLICore<Arc<Server>>,
    prompt: Option<ColoredString>,
    help_msg: ColoredString,
    term: Term,
    prompt_len: usize,
}

impl CommandLineInterface {
    pub fn new(builder: CLIBuilder) -> Self {
        builder.build()
    }

    /*
    pub fn await_input<F: Fn(String) -> anyhow::Result<bool>>(&self, handle_input: F) -> anyhow::Result<bool> {
        let input = input(&self.prompt)?;
        handle_input(input)
    }

    fn try_execute(&self, input: String) -> anyhow::Result<bool> {
        let mut parts = input.split(" ").collect::<Vec<_>>();
        let cmd = parts.remove(0).to_lowercase();

        match self.cmds.get(&cmd) {
            None => Ok(false),
            Some(cmd) => {
                cmd.cmd_impl.execute(self, &parts)?;
                Ok(true)
            },
        }
    }*/

    pub fn await_input(&self, server: &Arc<Server>) -> anyhow::Result<bool> {
        let input = if let Some(prompt) = &self.prompt {
            self.term.read_line_initial_text(format!("{}: ", prompt).as_str())?.split_off(self.prompt_len)
        } else {
            self.term.read_line()?
        };

        match self.core.process(server, input.as_str()) {
            Ok(_) => {
                Ok(true)
            }
            Err(err) => {
                match err {
                    InputError::CommandNotFound { .. } => {
                        server.println(format!("{}", self.help_msg).as_str());
                    }
                    _ => {
                        server.println(format!("{}", err).as_str());
                    }
                }
                Ok(false)
            }
        }
    }

    #[inline(always)]
    pub(crate) fn cmds(&self) -> &HashMap<String, Command<Arc<Server>>> {
        self.core.cmds()
    }

    pub fn println(&self, msg: &str) {
        self.term.write_line(msg).unwrap();
    }
}

pub struct CLIBuilder {
    cmds: Vec<CommandBuilder<Arc<Server>>>,
    prompt: Option<ColoredString>,
    help_msg: Option<ColoredString>,
}

impl CLIBuilder {
    pub fn new() -> Self {
        Self {
            cmds: vec![],
            prompt: None,
            help_msg: None,
        }
    }

    pub fn command(mut self, cmd: CommandBuilder<Arc<Server>>) -> Self {
        self.cmds.push(cmd);
        self
    }

    pub fn prompt(mut self, prompt: ColoredString) -> Self {
        self.prompt = Some(prompt);
        self
    }

    pub fn help_msg(mut self, help_msg: ColoredString) -> Self {
        self.help_msg = Some(help_msg);
        self
    }

    pub fn build(self) -> CommandLineInterface {
        let prompt_len = self.prompt.as_ref().map_or(0, |prompt| format!("{}", prompt).len() + 2);
        CommandLineInterface {
            core: CLICore::new(self.cmds),
            prompt: self.prompt,
            help_msg: self.help_msg.expect("a help message has to be specified before a CLI can be built"),
            term: Term::stdout(),
            prompt_len,
        }
    }
}

