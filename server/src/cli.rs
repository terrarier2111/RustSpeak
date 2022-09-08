use std::collections::HashMap;
use std::ops::Range;
use colored::ColoredString;
use crate::utils::input;

// FIXME: maybe add tab completion

pub struct CommandLineInterface<'a> {
    cmds: HashMap<String, Command<'a>>,
    prompt: Option<ColoredString>,
}

impl CommandLineInterface {

    pub fn new(builder: CLIBuilder) -> Self {
        let cmds = {
          let mut result = HashMap::new();

            for cmd in builder.cmds {
                result.insert(cmd.name.clone(), cmd);
            }

            result
        };

        Self {
            cmds,
            prompt: builder.prompt,
        }
    }

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
    }

    #[inline(always)]
    pub fn cmds(&self) -> &HashMap<String, Command> {
        &self.cmds
    }

}

pub struct CLIBuilder<'a> {
    cmds: Vec<Command<'a>>,
    prompt: Option<ColoredString>,
}

impl CLIBuilder<'_> {

    pub fn new() -> Self {
        Self {
            cmds: vec![],
            prompt: None,
        }
    }

    pub fn command(mut self, cmd: CommandBuilder) -> Self {
        self.cmds.push(cmd.build());
        self
    }

    pub fn prompt(mut self, prompt: ColoredString) -> Self {
        self.prompt = Some(prompt);
        self
    }

}

struct Command<'a> {
    name: String,
    desc: Option<String>,
    // FIXME: add usage
    params: &'a [UsageTy],
    aliases: Vec<String>,
    cmd_impl: Box<dyn CommandImpl>,
}

impl Command<'_> {

    #[inline(always)]
    pub fn name(&self) -> &String {
        &self.name
    }

    #[inline(always)]
    pub fn desc(&self) -> &Option<String> {
        &self.desc
    }

}

pub trait CommandImpl {
    fn execute(&self, cli: &CommandLineInterface, input: &[&str]) -> anyhow::Result<()>;
}

pub struct CommandBuilder {
    name: Option<String>,
    desc: Option<String>,
    aliases: Vec<String>,
    cmd_impl: Option<Box<dyn CommandImpl>>,
}

impl CommandBuilder {

    pub fn new() -> Self {
        Self {
            name: None,
            desc: None,
            aliases: vec![],
            cmd_impl: None,
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_lowercase());
        self
    }

    pub fn desc(mut self, desc: &str) -> Self {
        self.desc = Some(desc.to_lowercase());
        self
    }

    pub fn add_alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_lowercase());
        self
    }

    pub fn add_aliases(mut self, aliases: &[&str]) -> Self {
        let mut aliases = aliases.iter().map(|alias| alias.to_lowercase()).collect::<Vec<_>>();
        self.aliases.append(&mut aliases);
        self
    }

    pub fn cmd_impl(mut self, cmd_impl: Box<dyn CommandImpl>) -> Self {
        self.cmd_impl = Some(cmd_impl);
        self
    }

    fn build(self) -> Command {
        Command {
            name: self.name.expect("a name is required for a command in order for it to be used"),
            desc: self.desc,
            aliases: self.aliases,
            cmd_impl: self.cmd_impl.expect("a command implementation is required for a command in order for it to be used"),
        }
    }

}

pub enum CommandParam<'a> {
    Int(CmdParamNumConstraints<'a, usize>),
    Float(CmdParamNumConstraints<'a, f64>),
    String(CmdParamStrConstraints<'a>),
}

pub enum CmdParamNumConstraints<'a, T> {
    Range(Range<T>),
    Variants(&'a [T]),
    None,
}

pub enum CmdParamStrConstraints<'a> {
    Variants(&'a [&'a str]),
    None,
}

pub struct UsageBuilder<'a> {
    prefix: &'a str,
    req: Vec<CommandParam<'a>>,
    opt: Vec<CommandParam<'a>>,
    opt_prefixed: Vec<CommandParam<'a>>,
}

impl UsageBuilder {

    pub fn optional_prefixed_prefix(mut self, prefix: &str) -> Self {
        self.prefix = prefix;
        self
    }

    pub fn required(mut self, param: CommandParam) -> Self {
        if !self.opt.is_empty() {
            panic!("");
        }
        self.req.push(param);
        self
    }

    pub fn optional(mut self, param: CommandParam) -> Self {
        self.opt.push(param);
        self
    }

    pub fn optional_prefixed(mut self, param: CommandParam) -> Self {
        self.opt_prefixed.push(param);
        self
    }

}
