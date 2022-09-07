pub struct CommandLineInterface {
    cmds: Vec<Command>,
}

impl CommandLineInterface {

    pub fn new(builder: CLIBuilder) -> Self {
        Self {
            cmds: builder.cmds,
        }
    }

    pub fn try_execute(&mut self, input: String) -> anyhow::Result<bool> {
        let parts = input.split(" ").collect::<Vec<_>>();
        todo!()
    }

}

pub struct CLIBuilder {
    cmds: Vec<Command>,
}

impl CLIBuilder {

    pub fn new() -> Self {
        Self {
            cmds: vec![],
        }
    }

    pub fn command(mut self, cmd: CommandBuilder) -> Self {
        self.cmds.push(cmd.build());
        self
    }

}

struct Command {
    name: String,
    desc: Option<String>,
    aliases: Vec<String>,
    cmd_impl: Box<dyn CommandImpl>,
}

pub trait CommandImpl {
    fn execute(&self, cli: &mut CommandLineInterface, input: String) -> anyhow::Result<()>;
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

    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn desc(mut self, desc: String) -> Self {
        self.desc = Some(desc);
        self
    }

    pub fn add_alias(mut self, alias: String) -> Self {
        self.aliases.push(alias);
        self
    }

    pub fn add_aliases(mut self, aliases: &[String]) -> Self {
        self.aliases.extend_from_slice(aliases);
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
