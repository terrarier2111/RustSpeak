use crate::utils::input;
use colored::ColoredString;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use crate::Server;

// FIXME: maybe add tab completion

pub struct CommandLineInterface {
    cmds: HashMap<String, Command>,
    prompt: Option<ColoredString>,
    help_msg: ColoredString,
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
        let input = input(&self.prompt)?;
        let mut parts = input.split(" ").collect::<Vec<_>>();
        let cmd = parts.remove(0).to_lowercase();

        match self.cmds.get(&cmd) {
            None => {
                println!("{}", self.help_msg);
                Ok(false)
            },
            Some(cmd) => {
                cmd.cmd_impl.execute(server, &parts)?;
                Ok(true)
            }
        }
    }

    #[inline(always)]
    pub(crate) fn cmds(&self) -> &HashMap<String, Command> {
        &self.cmds
    }
}

pub struct CLIBuilder {
    cmds: Vec<Command>,
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

    pub fn command(mut self, cmd: CommandBuilder) -> Self {
        self.cmds.push(cmd.build());
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
        CommandLineInterface {
            prompt: self.prompt,
            cmds: {
                let mut cmds = HashMap::new();

                for cmd in self.cmds.into_iter() {
                    cmds.insert(cmd.name.clone(), cmd);
                }
                cmds
            },
            help_msg: self.help_msg.expect("a help message has to be specified before a CLI can be built"),
        }
    }
}

pub(crate) struct Command {
    name: String,
    desc: Option<String>,
    params: Option<UsageBuilder<BuilderImmutable>>,
    aliases: Vec<String>,
    cmd_impl: Box<dyn CommandImpl>,
}

impl Command {
    #[inline(always)]
    pub fn name(&self) -> &String {
        &self.name
    }

    #[inline(always)]
    pub fn desc(&self) -> &Option<String> {
        &self.desc
    }

    #[inline(always)]
    pub fn params(&self) -> &Option<UsageBuilder<BuilderImmutable>> {
        &self.params
    }
}

pub trait CommandImpl: Send + Sync {
    fn execute(&self, server: &Arc<Server>, input: &[&str]) -> anyhow::Result<()>;
}

pub struct CommandBuilder {
    name: Option<String>,
    desc: Option<String>,
    params: Option<UsageBuilder<BuilderImmutable>>,
    aliases: Vec<String>,
    cmd_impl: Option<Box<dyn CommandImpl>>,
}

impl CommandBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            desc: None,
            params: None,
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

    pub fn params(mut self, params: UsageBuilder<BuilderMutable>) -> Self {
        self.params = Some(params.finish());
        self
    }

    pub fn add_alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_lowercase());
        self
    }

    pub fn add_aliases(mut self, aliases: &[&str]) -> Self {
        let mut aliases = aliases
            .iter()
            .map(|alias| alias.to_lowercase())
            .collect::<Vec<_>>();
        self.aliases.append(&mut aliases);
        self
    }

    pub fn cmd_impl(mut self, cmd_impl: Box<dyn CommandImpl>) -> Self {
        self.cmd_impl = Some(cmd_impl);
        self
    }

    fn build(self) -> Command {
        Command {
            name: self
                .name
                .expect("a name is required for a command in order for it to be used"),
            desc: self.desc,
            params: self.params,
            aliases: self.aliases,
            cmd_impl: self.cmd_impl.expect(
                "a command implementation is required for a command in order for it to be used",
            ),
        }
    }
}

pub struct CommandParam {
    pub name: String,
    pub ty: CommandParamTy,
}

pub enum CommandParamTy {
    Int(CmdParamNumConstraints<usize>),
    Float(CmdParamNumConstraints<f64>),
    String(CmdParamStrConstraints),
}

impl CommandParamTy {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandParamTy::Int(_) => "integer",
            CommandParamTy::Float(_) => "decimal",
            CommandParamTy::String(_) => "string", // FIXME: is there a better/more user friendly name for this?
        }
    }
}

impl Display for CommandParamTy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub enum CmdParamNumConstraints<T> {
    Range(Range<T>),
    Variants(Box<[T]>),
    None,
}

pub enum CmdParamStrConstraints {
    Variants(&'static [&'static str]),
    None,
}

pub struct BuilderMutable;
pub struct BuilderImmutable;

struct InnerBuilder {
    prefix: Option<String>,
    req: Vec<CommandParam>,
    opt: Vec<CommandParam>,
    opt_prefixed: Vec<CommandParam>,
}

pub struct UsageBuilder<M = BuilderMutable> {
    inner: InnerBuilder,
    mutability: PhantomData<M>,
}

impl<'a> UsageBuilder<BuilderMutable> {
    pub fn new() -> Self {
        Self {
            inner: InnerBuilder {
                prefix: None,
                req: vec![],
                opt: vec![],
                opt_prefixed: vec![],
            },
            mutability: Default::default(),
        }
    }

    pub fn optional_prefixed_prefix(mut self, prefix: String) -> Self {
        self.inner.prefix = Some(prefix);
        self
    }

    pub fn required(mut self, param: CommandParam) -> Self {
        if !self.inner.opt.is_empty() {
            panic!("you can only append required parameters before any optional parameters get appended");
        }
        self.inner.req.push(param);
        self
    }

    pub fn optional(mut self, param: CommandParam) -> Self {
        self.inner.opt.push(param);
        self
    }

    pub fn optional_prefixed(mut self, param: CommandParam) -> Self {
        if self.inner.prefix.is_none() {
            panic!("a prefix has to be specified in order to add optional prefixed parameters");
        }
        self.inner.opt_prefixed.push(param);
        self
    }

    fn finish(self) -> UsageBuilder<BuilderImmutable> {
        UsageBuilder {
            inner: self.inner,
            mutability: Default::default(),
        }
    }
}

impl UsageBuilder<BuilderImmutable> {
    #[inline(always)]
    pub fn optional_prefixed_prefix(&self) -> &Option<String> {
        &self.inner.prefix
    }

    #[inline(always)]
    pub fn required(&self) -> &Vec<CommandParam> {
        &self.inner.req
    }

    #[inline(always)]
    pub fn optional(&self) -> &Vec<CommandParam> {
        &self.inner.opt
    }

    #[inline(always)]
    pub fn optional_prefixed(&self) -> &Vec<CommandParam> {
        &self.inner.opt_prefixed
    }
}
