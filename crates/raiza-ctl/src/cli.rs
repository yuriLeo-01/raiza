//! Definição da linha de comando (clap) e conversão para [`CapabilityValue`].
//!
//! Nada aqui faz I/O: dá para testar só com `Cli::try_parse_from`.
//!
//! Os comentários `///` documentam o código. O texto de ajuda que o usuário vê vem de
//! [`help_text`], traduzido pelo `raiza-i18n` e aplicado por [`localized_command`].

use clap::{ArgGroup, Command as ClapCommand, CommandFactory, Parser, Subcommand, ValueEnum};
use raiza_core::{CapabilityValue, Color, DeviceId, RgbSetting};
use raiza_i18n::gettext;

/// Hub de periféricos para Linux: fala com o daemon `raizad` pelo D-Bus.
#[derive(Debug, Parser)]
#[command(name = "raiza-ctl", version, about)]
pub struct Cli {
    /// O que fazer.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcomandos de nível superior.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lista os dispositivos conhecidos pelo daemon
    List {
        /// Imprime o JSON do daemon, para scripts (sempre com todos os aparelhos)
        #[arg(long)]
        json: bool,
        /// Mostra também os aparelhos sem controles (que só dizem "conectado")
        #[arg(long)]
        all: bool,
    },
    /// Mostra todas as capacidades de um dispositivo
    Get {
        /// Id do dispositivo (ex.: usb:24ae:1423:0)
        id: DeviceId,
        /// Imprime o JSON do daemon, para scripts
        #[arg(long)]
        json: bool,
    },
    /// Altera uma capacidade de um dispositivo
    Set {
        /// Id do dispositivo (ex.: usb:24ae:1423:0)
        id: DeviceId,
        /// Capacidade e novo valor
        #[command(subcommand)]
        value: SetCommand,
    },
    /// Acompanha os eventos do daemon em tempo real (um por linha)
    Watch {
        /// Imprime um objeto JSON por linha
        #[arg(long)]
        json: bool,
    },
    /// Gera o script de autocompletar para o seu shell
    Completions {
        /// Shell de destino
        shell: CompletionShell,
    },
}

/// Shells aceitos em `completions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    /// Bash
    Bash,
    /// Zsh
    Zsh,
    /// Fish
    Fish,
}

/// `on` ou `off`. O `ValueEnum` é à mão para a ajuda de cada valor ser traduzida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnOff {
    /// Ligado.
    On,
    /// Desligado.
    Off,
}

impl ValueEnum for OnOff {
    fn value_variants<'a>() -> &'a [Self] {
        &[OnOff::On, OnOff::Off]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        let (name, help) = match self {
            OnOff::On => ("on", gettext("On")),
            OnOff::Off => ("off", gettext("Off")),
        };
        Some(clap::builder::PossibleValue::new(name).help(help))
    }
}

impl OnOff {
    fn as_bool(self) -> bool {
        self == OnOff::On
    }
}

/// Uma capacidade e o valor novo, como se escreve no `set`.
#[derive(Debug, Subcommand)]
pub enum SetCommand {
    /// Resolução do sensor (DPI)
    Dpi {
        /// Novo DPI
        value: u32,
    },
    /// Um estágio da tabela de DPI (o em uso ou outro)
    DpiStage {
        /// Índice do estágio, a partir de 0
        stage: u32,
        /// Novo DPI do estágio
        value: u32,
    },
    /// Taxa de atualização em Hz
    PollingRate {
        /// Nova taxa em Hz
        value: u32,
    },
    /// Taxa de varredura (Key Scan Rate) em Hz
    ScanRate {
        /// Nova taxa em Hz
        value: u32,
    },
    /// Volume em porcentagem
    Volume {
        /// Novo volume (%)
        value: u32,
    },
    /// Liga ou desliga o mudo
    Mute {
        /// `on` muda, `off` tira do mudo
        state: OnOff,
    },
    /// Conecta ou desconecta o dispositivo
    Connection {
        /// `on` conecta, `off` desconecta
        state: OnOff,
    },
    /// Iluminação RGB (informe pelo menos --mode ou --color)
    #[command(group(
        ArgGroup::new("alteracao").required(true).multiple(true).args(["mode", "color"])
    ))]
    Rgb {
        /// Índice da zona; sem ele, vale para todas
        #[arg(long)]
        zone: Option<u32>,
        /// Nome do modo (ex.: static)
        #[arg(long)]
        mode: Option<String>,
        /// Cor no formato #rrggbb
        #[arg(long, value_name = "#RRGGBB")]
        color: Option<Color>,
    },
    /// Dispara um pulso de vibração
    Rumble {
        /// Intensidade de 0 a 100
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        strength: u8,
        /// Duração em milissegundos
        duration_ms: u32,
    },
}

impl SetCommand {
    /// Converte o que foi digitado no valor que o daemon espera.
    ///
    /// Só falha no `rgb` sem `--mode` nem `--color` (o clap já impede isso na linha de
    /// comando; a checagem aqui protege quem monta o comando à mão).
    pub fn to_value(&self) -> Result<CapabilityValue, String> {
        Ok(match self {
            SetCommand::Dpi { value } => CapabilityValue::Dpi(*value),
            SetCommand::DpiStage { stage, value } => CapabilityValue::DpiStage {
                stage: *stage,
                value: *value,
            },
            SetCommand::PollingRate { value } => CapabilityValue::PollingRate(*value),
            SetCommand::ScanRate { value } => CapabilityValue::ScanRate(*value),
            SetCommand::Volume { value } => CapabilityValue::Volume(*value),
            SetCommand::Mute { state } => CapabilityValue::Mute(state.as_bool()),
            SetCommand::Connection { state } => CapabilityValue::Connection(state.as_bool()),
            SetCommand::Rgb { zone, mode, color } => {
                if mode.is_none() && color.is_none() {
                    return Err(gettext("rgb: give at least --mode or --color").into());
                }
                CapabilityValue::Rgb(RgbSetting {
                    zone: *zone,
                    mode: mode.clone(),
                    color: *color,
                })
            }
            SetCommand::Rumble {
                strength,
                duration_ms,
            } => CapabilityValue::Rumble {
                strength: *strength,
                duration_ms: *duration_ms,
            },
        })
    }
}

/// Texto de ajuda traduzido do comando em `path` (subcomandos separados por espaço, `""`
/// para a raiz) ou, com `arg`, de um argumento dele.
fn help_text(path: &str, arg: Option<&str>) -> Option<&'static str> {
    // O subcomando `help` que o clap cria em cada nível. Os filhos dele só repetem os
    // nomes dos subcomandos, sem texto.
    if is_inside_help(path) {
        return None;
    }
    if arg.is_none() && (path == "help" || path.ends_with(" help")) {
        return Some(gettext(
            "Print this message or the help of the given subcommand(s)",
        ));
    }
    let text = match (path, arg) {
        // Argumentos que o clap cria sozinho, em todo comando.
        (_, Some("help")) => gettext("Print help"),
        (_, Some("version")) => gettext("Print version"),

        ("", None) => gettext("Peripheral hub for Linux: talks to the raizad daemon over D-Bus"),
        ("list", None) => gettext("List the devices known to the daemon"),
        ("list", Some("json")) => {
            gettext("Print the daemon JSON, for scripts (always with every device)")
        }
        ("list", Some("all")) => {
            gettext("Also show devices without controls (that only say \"connected\")")
        }
        ("get", None) => gettext("Show every capability of a device"),
        ("get" | "set", Some("id")) => gettext("Device id (e.g. usb:24ae:1423:0)"),
        ("get", Some("json")) => gettext("Print the daemon JSON, for scripts"),
        ("set", None) => gettext("Change a capability of a device"),
        ("set", Some("value")) => gettext("Capability and new value"),
        ("watch", None) => gettext("Follow the daemon events in real time (one per line)"),
        ("watch", Some("json")) => gettext("Print one JSON object per line"),
        ("completions", None) => gettext("Generate the completion script for your shell"),
        ("completions", Some("shell")) => gettext("Target shell"),

        ("set dpi", None) => gettext("Sensor resolution (DPI)"),
        ("set dpi", Some("value")) => gettext("New DPI"),
        ("set dpi-stage", None) => {
            gettext("One stage of the DPI table (the one in use or another)")
        }
        ("set dpi-stage", Some("stage")) => gettext("Stage index, starting at 0"),
        ("set dpi-stage", Some("value")) => gettext("New DPI of the stage"),
        ("set polling-rate", None) => gettext("Polling rate in Hz"),
        ("set polling-rate" | "set scan-rate", Some("value")) => gettext("New rate in Hz"),
        ("set scan-rate", None) => gettext("Key Scan Rate in Hz"),
        ("set volume", None) => gettext("Volume in percent"),
        ("set volume", Some("value")) => gettext("New volume (%)"),
        ("set mute", None) => gettext("Turn mute on or off"),
        ("set mute", Some("state")) => gettext("`on` mutes, `off` unmutes"),
        ("set connection", None) => gettext("Connect or disconnect the device"),
        ("set connection", Some("state")) => gettext("`on` connects, `off` disconnects"),
        ("set rgb", None) => gettext("RGB lighting (give at least --mode or --color)"),
        ("set rgb", Some("zone")) => gettext("Zone index; without it, applies to all zones"),
        ("set rgb", Some("mode")) => gettext("Mode name (e.g. static)"),
        ("set rgb", Some("color")) => gettext("Color as #rrggbb"),
        ("set rumble", None) => gettext("Send a rumble pulse"),
        ("set rumble", Some("strength")) => gettext("Strength from 0 to 100"),
        ("set rumble", Some("duration_ms")) => gettext("Duration in milliseconds"),
        _ => return None,
    };
    Some(text)
}

/// Comando abaixo de um subcomando `help` (`help set`, `set help dpi`).
fn is_inside_help(path: &str) -> bool {
    path.starts_with("help ") || path.contains(" help ")
}

/// A linha de comando com a ajuda na língua do usuário: textos de [`help_text`] e
/// títulos das seções. Mensagens de erro do próprio clap continuam em inglês.
pub fn localized_command() -> ClapCommand {
    let mut cmd = Cli::command();
    // Monta os argumentos automáticos (`--help`, `--version`, subcomando `help`) para
    // poder traduzi-los também.
    cmd.build();
    localize(cmd, "")
}

fn localize(mut cmd: ClapCommand, path: &str) -> ClapCommand {
    let template = format!(
        "{{before-help}}{{about-with-newline}}\n{} {{usage}}\n\n{{all-args}}{{after-help}}",
        gettext("Usage:")
    );
    cmd = cmd
        .help_template(template)
        .subcommand_help_heading(gettext("Commands"));
    if let Some(about) = help_text(path, None) {
        cmd = cmd.about(about);
    }
    let args: Vec<(clap::Id, bool)> = cmd
        .get_arguments()
        .map(|a| (a.get_id().clone(), a.is_positional()))
        .collect();
    for (id, positional) in args {
        let help = help_text(path, Some(id.as_str()));
        cmd = cmd.mut_arg(id, |mut a| {
            if let Some(help) = help {
                a = a.help(help);
            }
            a.help_heading(if positional {
                gettext("Arguments")
            } else {
                gettext("Options")
            })
        });
    }
    let subs: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_owned())
        .collect();
    for name in subs {
        let sub_path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path} {name}")
        };
        cmd = cmd.mut_subcommand(name, |s| localize(s, &sub_path));
    }
    cmd
}

/// Escreve o script de autocompletar de `shell` em `out`.
pub fn completions(shell: CompletionShell, out: &mut dyn std::io::Write) -> std::io::Result<()> {
    use clap_complete::{Shell, generate};

    let shell = match shell {
        CompletionShell::Bash => Shell::Bash,
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };
    // Gera num buffer: o `generate` entra em pânico se o pipe fechar no meio da escrita.
    let mut buf = Vec::new();
    generate(shell, &mut localized_command(), "raiza-ctl", &mut buf);
    out.write_all(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "usb:24ae:1423:0";

    /// Converte `set <ID> <args…>` em valor, ou devolve o erro do clap.
    fn set(args: &[&str]) -> Result<CapabilityValue, String> {
        let mut argv = vec!["raiza-ctl", "set", ID];
        argv.extend_from_slice(args);
        let cli = Cli::try_parse_from(argv).map_err(|e| e.to_string())?;
        match cli.command {
            Command::Set { id, value } => {
                assert_eq!(id.to_string(), ID);
                value.to_value()
            }
            outro => panic!("esperava set, veio {outro:?}"),
        }
    }

    #[test]
    fn set_numericos() {
        assert_eq!(set(&["dpi", "1600"]), Ok(CapabilityValue::Dpi(1600)));
        assert_eq!(
            set(&["polling-rate", "1000"]),
            Ok(CapabilityValue::PollingRate(1000))
        );
        assert_eq!(set(&["volume", "40"]), Ok(CapabilityValue::Volume(40)));
        assert_eq!(
            set(&["dpi-stage", "2", "950"]),
            Ok(CapabilityValue::DpiStage {
                stage: 2,
                value: 950
            })
        );
        assert_eq!(
            set(&["scan-rate", "4000"]),
            Ok(CapabilityValue::ScanRate(4000))
        );
    }

    #[test]
    fn set_liga_desliga() {
        assert_eq!(set(&["mute", "on"]), Ok(CapabilityValue::Mute(true)));
        assert_eq!(set(&["mute", "off"]), Ok(CapabilityValue::Mute(false)));
        assert_eq!(
            set(&["connection", "on"]),
            Ok(CapabilityValue::Connection(true))
        );
        assert_eq!(
            set(&["connection", "off"]),
            Ok(CapabilityValue::Connection(false))
        );
    }

    #[test]
    fn set_rgb() {
        let cor = Color { r: 0, g: 255, b: 0 };
        assert_eq!(
            set(&[
                "rgb", "--zone", "1", "--mode", "static", "--color", "#00ff00"
            ]),
            Ok(CapabilityValue::Rgb(RgbSetting {
                zone: Some(1),
                mode: Some("static".into()),
                color: Some(cor),
            }))
        );
        assert_eq!(
            set(&["rgb", "--color", "#00ff00"]),
            Ok(CapabilityValue::Rgb(RgbSetting {
                zone: None,
                mode: None,
                color: Some(cor),
            }))
        );
        assert_eq!(
            set(&["rgb", "--mode", "wave"]),
            Ok(CapabilityValue::Rgb(RgbSetting {
                zone: None,
                mode: Some("wave".into()),
                color: None,
            }))
        );
    }

    #[test]
    fn set_rumble() {
        assert_eq!(
            set(&["rumble", "80", "500"]),
            Ok(CapabilityValue::Rumble {
                strength: 80,
                duration_ms: 500
            })
        );
    }

    #[test]
    fn set_invalidos() {
        // Falta o valor, valor que não é número, negativo.
        assert!(set(&["dpi"]).is_err());
        assert!(set(&["dpi", "alto"]).is_err());
        assert!(set(&["volume", "-5"]).is_err());
        assert!(set(&["polling-rate", "1000", "2000"]).is_err());
        // Capacidade desconhecida (inclusive a forma com sublinhado).
        assert!(set(&["brilho", "3"]).is_err());
        assert!(set(&["polling_rate", "1000"]).is_err());
        // on/off.
        assert!(set(&["mute", "talvez"]).is_err());
        assert!(set(&["mute"]).is_err());
        assert!(set(&["connection", "1"]).is_err());
        // rgb: nada informado, cor ruim, zona ruim.
        assert!(set(&["rgb"]).is_err());
        assert!(set(&["rgb", "--zone", "0"]).is_err());
        assert!(set(&["rgb", "--color", "verde"]).is_err());
        assert!(set(&["rgb", "--color", "00ff00"]).is_err());
        assert!(set(&["rgb", "--zone", "x", "--mode", "static"]).is_err());
        // rumble: intensidade acima de 100, argumentos faltando.
        assert!(set(&["rumble", "101", "500"]).is_err());
        assert!(set(&["rumble", "50"]).is_err());
        assert!(set(&["rumble", "50", "-1"]).is_err());
    }

    #[test]
    fn id_invalido() {
        assert!(Cli::try_parse_from(["raiza-ctl", "set", "lixo", "dpi", "800"]).is_err());
        assert!(Cli::try_parse_from(["raiza-ctl", "get", "lixo"]).is_err());
        assert!(Cli::try_parse_from(["raiza-ctl", "get"]).is_err());
    }

    #[test]
    fn rgb_sem_nada_na_mao_e_recusado() {
        let vazio = SetCommand::Rgb {
            zone: Some(0),
            mode: None,
            color: None,
        };
        assert!(vazio.to_value().is_err());
    }

    #[test]
    fn outros_subcomandos() {
        let json = |args: &[&str]| {
            let mut v = vec!["raiza-ctl"];
            v.extend_from_slice(args);
            match Cli::try_parse_from(v).unwrap().command {
                Command::List { json, .. }
                | Command::Watch { json }
                | Command::Get { json, .. } => json,
                _ => panic!("subcomando inesperado"),
            }
        };
        assert!(!json(&["list"]));
        assert!(json(&["list", "--json"]));
        assert!(json(&["get", ID, "--json"]));
        assert!(json(&["watch", "--json"]));
        assert!(Cli::try_parse_from(["raiza-ctl", "completions", "fish"]).is_ok());
        assert!(Cli::try_parse_from(["raiza-ctl", "completions", "powershell"]).is_err());
        assert!(Cli::try_parse_from(["raiza-ctl"]).is_err());
    }

    /// Todo comando e argumento tem texto na tabela [`help_text`]: sem isso, a ajuda
    /// mostraria o comentário `///` em português, fora da tradução.
    #[test]
    fn toda_ajuda_vem_da_tabela() {
        fn confere(cmd: &ClapCommand, path: &str, faltando: &mut Vec<String>) {
            if is_inside_help(path) {
                return;
            }
            if help_text(path, None).is_none() {
                faltando.push(format!("{path:?}"));
            }
            for a in cmd.get_arguments() {
                if help_text(path, Some(a.get_id().as_str())).is_none() {
                    faltando.push(format!("{path:?} {}", a.get_id()));
                }
            }
            for s in cmd.get_subcommands() {
                let sub = if path.is_empty() {
                    s.get_name().to_owned()
                } else {
                    format!("{path} {}", s.get_name())
                };
                confere(s, &sub, faltando);
            }
        }
        let mut cmd = Cli::command();
        cmd.build();
        let mut faltando = Vec::new();
        confere(&cmd, "", &mut faltando);
        assert!(faltando.is_empty(), "sem texto de ajuda: {faltando:?}");
    }

    #[test]
    fn ajuda_traduzida_e_parse_continuam_valendo() {
        let mut cmd = localized_command();
        let ajuda = cmd.render_help().to_string();
        assert!(ajuda.contains("Peripheral hub for Linux"), "{ajuda}");
        assert!(ajuda.contains("Usage: raiza-ctl"), "{ajuda}");
        assert!(
            ajuda.contains("Commands:") && ajuda.contains("Options:"),
            "{ajuda}"
        );
        assert!(ajuda.contains("List the devices known to the daemon"));
        let set = cmd.find_subcommand_mut("set").unwrap();
        let mute = set.find_subcommand_mut("mute").unwrap();
        let ajuda = mute.render_long_help().to_string();
        assert!(
            ajuda.contains("Arguments:") && ajuda.contains("unmutes"),
            "{ajuda}"
        );
        assert!(ajuda.contains("on") && ajuda.contains("On"), "{ajuda}");
        assert!(!ajuda.contains("Ligado"), "{ajuda}");

        // O comando traduzido continua convertendo para `Cli`.
        use clap::FromArgMatches;
        let m = localized_command()
            .try_get_matches_from(["raiza-ctl", "set", "usb:24ae:1423:0", "mute", "on"])
            .unwrap();
        let cli = Cli::from_arg_matches(&m).unwrap();
        match cli.command {
            Command::Set { value, .. } => {
                assert_eq!(value.to_value(), Ok(CapabilityValue::Mute(true)));
            }
            outro => panic!("esperava set, veio {outro:?}"),
        }
    }

    #[test]
    fn completions_geram_script() {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
        ] {
            let mut buf = Vec::new();
            completions(shell, &mut buf).unwrap();
            assert!(String::from_utf8(buf).unwrap().contains("raiza-ctl"));
        }
    }
}
