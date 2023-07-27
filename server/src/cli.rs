use crate::utils::input;
use colored::ColoredString;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter, Write};
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use console::Term;
use crate::Server;

// FIXME: maybe add tab completion

pub struct CommandLineInterface {
    cmds: HashMap<String, Command>,
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
        let mut parts = input.split(" ").collect::<Vec<_>>();
        let cmd = parts.remove(0).to_lowercase();

        match self.cmds.get(&cmd) {
            None => {
                server.println(format!("{}", self.help_msg).as_str());
                Ok(false)
            },
            Some(cmd) => {
                if cmd.params.as_ref().map(|all| all.inner.req.len()).unwrap_or(0) > parts.len() {
                    // FIXME: print help properly!
                    server.println("Too few parameters!");
                    return Ok(false);
                }
                cmd.cmd_impl.execute(server, &parts)?;
                Ok(true)
            }
        }
    }

    #[inline(always)]
    pub(crate) fn cmds(&self) -> &HashMap<String, Command> {
        &self.cmds
    }

    pub fn println(&self, msg: &str) {
        self.term.write_line(msg).unwrap();
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
        let prompt_len = self.prompt.as_ref().map_or(0, |prompt| format!("{}", prompt).len() + 2);
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
            term: Term::stdout(),
            prompt_len,
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

impl CommandParam {

    fn to_string(&self, indent: usize) -> String {
        format!("{}({})", self.name.as_str(), self.ty.to_string(indent))
    }

}

pub enum CommandParamTy {
    Int(CmdParamNumConstraints<usize>),
    Float(CmdParamDecimalConstraints<f64>),
    String(CmdParamStrConstraints),
    Enum(Vec<(&'static str, EnumVal)>),
}

impl CommandParamTy {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandParamTy::Int(_) => "integer",
            CommandParamTy::Float(_) => "decimal",
            CommandParamTy::String(_) => "string", // FIXME: is there a better/more user friendly name for this?
            CommandParamTy::Enum(_) => "enum",
        }
    }

    pub fn to_string(&self, indents: usize) -> String {
        match self {
            CommandParamTy::Int(constraints) => match constraints {
                CmdParamNumConstraints::Range(range) => format!("int({} to {})", range.start, range.end),
                CmdParamNumConstraints::Variants(variants) => {
                    let mut finished = String::from("int(");
                    let mut variants = variants.iter();
                    if let Some(variant) = variants.next() {
                        finished.push_str(format!("{}", variant).as_str());
                        for variant in variants {
                            finished.push_str(format!(", {}", variant).as_str());
                        }
                    }
                    finished.push(')');
                    finished
                }
                CmdParamNumConstraints::None => String::from("int"),
            },
            CommandParamTy::Float(constraints) => match constraints {
                CmdParamDecimalConstraints::Range(range) => format!("decimal({} to {})", range.start, range.end),
                CmdParamDecimalConstraints::None => String::from("decimal"),
            },
            CommandParamTy::String(constraints) => match constraints {
                CmdParamStrConstraints::Range(range) => format!("string(length {} to {})", range.start, range.end),
                CmdParamStrConstraints::None => String::from("string"),
            },
            CommandParamTy::Enum(variants) => {
                let mut finished = String::from("variants:\r\n");
                for variant in variants.iter() {
                    finished.push_str(" ".repeat(indents).as_str());
                    finished.push_str("- \"");
                    finished.push_str(variant.0);
                    match &variant.1 {
                        EnumVal::Simple(ty) => {
                            finished.push_str("\"(");
                            finished.push_str(ty.to_string(indents + 1).as_str());
                            finished.push(')');
                        }
                        EnumVal::Complex(params) => {
                            finished.push_str("\": ");
                            finished.push_str(params.to_string(indents + 1).as_str());
                        }
                        EnumVal::None => {
                            finished.push('\"');
                        }
                    }
                    finished.push_str("\r\n");
                }
                if !variants.is_empty() {
                    finished.push_str(" ".repeat(indents).as_str());
                }
                finished
            },
        }
    }
}

pub enum EnumVal {
    Simple(CommandParamTy),
    Complex(UsageSubBuilder<BuilderMutable>),
    None,
}

pub enum CmdParamNumConstraints<T> {
    Range(Range<T>),
    Variants(Box<[T]>),
    None,
}

pub enum CmdParamDecimalConstraints<T> {
    Range(Range<T>),
    None,
}

pub enum CmdParamStrConstraints {
    Range(Range<usize>),
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

struct InnerSubBuilder {
    prefix: Option<String>,
    req: Vec<CommandParam>,
    opt_prefixed: Vec<CommandParam>,
}

pub struct UsageSubBuilder<M = BuilderMutable> {
    inner: InnerSubBuilder,
    mutability: PhantomData<M>,
}

impl<'a> UsageSubBuilder<BuilderMutable> {
    pub fn new() -> Self {
        Self {
            inner: InnerSubBuilder {
                prefix: None,
                req: vec![],
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
        self.inner.req.push(param);
        self
    }

    pub fn optional_prefixed(mut self, param: CommandParam) -> Self {
        if self.inner.prefix.is_none() {
            panic!("a prefix has to be specified in order to add optional prefixed parameters");
        }
        self.inner.opt_prefixed.push(param);
        self
    }

    fn finish(self) -> UsageSubBuilder<BuilderImmutable> {
        UsageSubBuilder {
            inner: self.inner,
            mutability: Default::default(),
        }
    }
}

impl UsageSubBuilder<BuilderImmutable> {
    #[inline(always)]
    pub fn optional_prefixed_prefix(&self) -> &Option<String> {
        &self.inner.prefix
    }

    #[inline(always)]
    pub fn required(&self) -> &Vec<CommandParam> {
        &self.inner.req
    }

    #[inline(always)]
    pub fn optional_prefixed(&self) -> &Vec<CommandParam> {
        &self.inner.opt_prefixed
    }
}

impl<M> UsageSubBuilder<M> {

    fn to_string(&self, indents: usize) -> String {
        let mut finished = String::new();
        let mut req = self.inner.req.iter();
        if let Some(req_first) = req.next() {
            finished.push_str(req_first.to_string(indents).as_str());
            for req in req {
                finished.push(' ');
                finished.push_str(req.to_string(indents).as_str());
            }
        }
        let mut opt = self.inner.opt_prefixed.iter();
        if let Some(opt_first) = opt.next() {
            finished.push_str(self.inner.prefix.as_ref().unwrap().as_str());
            finished.push_str(opt_first.to_string(indents).as_str());
            for opt in opt {
                finished.push(' ');
                finished.push_str(self.inner.prefix.as_ref().unwrap().as_str());
                finished.push_str(opt.to_string(indents).as_str());
            }
        }
        finished
    }

}
