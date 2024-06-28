use bpaf::{Bpaf, Parser};
use kdl::{KdlDocument, KdlNode, KdlValue};
use miette::{bail, miette, Context, IntoDiagnostic};
use std::{
    collections::HashMap,
    fmt::{self, Write},
    io::Read,
    path::PathBuf,
    str::FromStr,
};

#[derive(Bpaf)]
struct Opts {
    env: bool,
    bind: Vec<Binding>,
    #[bpaf(positional)]
    file: Option<PathBuf>,
}

struct Binding {
    ident: String,
    value: String,
}
impl FromStr for Binding {
    type Err = miette::Error;
    fn from_str(s: &str) -> miette::Result<Self> {
        let (ident, value) = s
            .split_once('=')
            .ok_or_else(|| miette!("Binding needs an equals sign"))?;
        Ok(Binding {
            ident: ident.to_owned(),
            value: value.to_owned(),
        })
    }
}

fn main() -> miette::Result<()> {
    let opts = opts().run();
    let mut bindings: HashMap<String, String> = HashMap::default();
    if opts.env {
        for (var, val) in std::env::vars() {
            bindings.insert(var.to_owned(), val.to_owned());
        }
    }
    for binding in opts.bind {
        bindings.insert(binding.ident, binding.value);
    }
    let mut context = Ctx {
        bindings,
        wdir: opts
            .file
            .as_ref()
            .and_then(|path| path.parent())
            .map(|x| x.to_owned())
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let kdl_source = match &opts.file {
        Some(path) => std::fs::read_to_string(path)
            .into_diagnostic()
            .context(path.display().to_string())?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .into_diagnostic()?;
            buf
        }
    };
    let doc: KdlDocument = kdl_source.parse()?;
    let mut output = String::new();
    for node in doc.nodes() {
        handle_node(&mut output, node, 0, &mut context)?;
    }
    println!("{output}");
    Ok(())
}

fn interpolate_txt(context: &Ctx, txt: &str) -> miette::Result<String> {
    let mut xs = txt.split("${");
    let mut out = xs.next().unwrap().to_owned();
    for x in xs {
        match x.split_once('}') {
            Some((var, rem)) => {
                let value = context
                    .bindings
                    .get(var)
                    .ok_or_else(|| miette!("{var}: Variable not found\n{:?}", context))?;
                out.push_str(value);
                out.push_str(rem);
            }
            None => bail!("Unclosed interpolation"),
        }
    }
    Ok(out)
}

fn value_to_string(val: &KdlValue) -> String {
    val.as_string()
        .map(|x| x.to_owned())
        .unwrap_or_else(|| val.to_string())
}

fn handle_node(
    output: &mut String,
    node: &KdlNode,
    depth: usize,
    context: &mut Ctx,
) -> miette::Result<()> {
    match NodeType::infer(node)? {
        NodeType::Text => {
            let txt = get_text_arg(node).ok_or_else(|| miette!("No text args"))?;
            let txt = interpolate_txt(context, txt)?;
            // FIXME: What if you don't want newlines?
            writeln!(output, "{}{}", Indent(depth), txt).unwrap();
        }
        NodeType::BindVar => {
            for e in node.entries() {
                if let Some(name) = e.name() {
                    let value = value_to_string(e.value());
                    context.bindings.insert(name.to_string(), value.to_string());
                    // eprintln!("Bound {name} = {value}");
                }
            }
        }
        NodeType::Doctype => {
            if depth == 0 {
                let x = get_text_arg(node).ok_or_else(|| miette!("Doctype needs an arg"))?;
                writeln!(output, "<!DOCTYPE {}>", x).unwrap();
            } else {
                bail!("doctype is only valid at the document root");
            }
        }
        NodeType::Html5 => {
            write!(output, "{}<{}", Indent(depth), node.name().value()).unwrap();
            for arg in node.entries() {
                if let Some(name) = arg.name() {
                    let value = interpolate_txt(context, arg.value().as_string().unwrap())?;
                    write!(output, " {name}=\"{value}\"").unwrap();
                }
            }
            if let Some(txt) = get_text_arg(node) {
                let txt = interpolate_txt(context, txt)?;
                writeln!(output, ">{}</{}>", txt, node.name().value()).unwrap();
            } else if let Some(doc) = node.children() {
                writeln!(output, ">").unwrap();
                for child in doc.nodes() {
                    handle_node(output, child, depth + 1, context)?;
                }
                writeln!(output, "{}</{}>", Indent(depth), node.name().value()).unwrap();
            } else {
                writeln!(output, " />").unwrap();
            }
        }
        NodeType::Include => {
            let path = get_text_arg(node).ok_or_else(|| miette!("No arg"))?;
            let path = interpolate_txt(context, path)?;
            let path = context.wdir.join(path);
            let source = std::fs::read_to_string(&path)
                .into_diagnostic()
                .context(path.display().to_string())?;
            let ext = path.extension().and_then(|x| x.to_str());
            match ext {
                Some("html") => {
                    for line in source.lines() {
                        writeln!(output, "{}{}", Indent(depth), line).unwrap();
                    }
                }
                Some("md") | Some("markdown") => {
                    let parser = pulldown_cmark::Parser::new(&source);
                    let mut html_output = String::new();
                    pulldown_cmark::html::push_html(&mut html_output, parser);
                    for line in html_output.lines() {
                        writeln!(output, "{}{}", Indent(depth), line).unwrap();
                    }
                }
                Some("kdl") => {
                    let doc: KdlDocument = source.parse()?;
                    let mut bindings = context.bindings.clone();
                    for arg in node.entries() {
                        if let Some(x) = arg.name() {
                            bindings.insert(x.value().to_owned(), value_to_string(arg.value()));
                        }
                    }
                    let mut context2 = Ctx {
                        bindings,
                        wdir: path
                            .parent()
                            .map(|x| x.to_owned())
                            .unwrap_or_else(|| PathBuf::from(".")),
                    };
                    for node in doc.nodes() {
                        handle_node(output, node, depth, &mut context2)?;
                    }
                }
                Some(ext) => bail!("{ext}: Extension not recognised"),
                None => bail!("No file extension"),
            };
        } // NodeType::InlineMarkdown => {
          //     let md_source =
          //         if let Some(path) = node.get("include").and_then(|x| x.value().as_string()) {
          //             std::fs::read_to_string(path)?
          //         } else if !node.entries().is_empty() {
          //             node.entries()
          //                 .iter()
          //                 .filter(|x| x.name().is_none())
          //                 .filter_map(|x| x.value().as_string())
          //                 .collect()
          //         } else {
          //             node.children()
          //                 .iter()
          //                 .flat_map(|x| x.nodes())
          //                 .map(|x| format!("{}\n", x.name().value()))
          //                 .collect()
          //         };
          //     let parser = pulldown_cmark::Parser::new(&md_source);
          //     let mut html_output = String::new();
          //     pulldown_cmark::html::push_html(&mut html_output, parser);
          //     for line in html_output.lines() {
          //         writeln!(output, "{}{}", Indent(depth).unwrap() line)?;
          //     }
          // }
    }
    Ok(())
}

enum NodeType {
    Doctype,
    Html5,
    Text,
    BindVar,
    Include,
}

#[derive(Debug)]
struct Ctx {
    bindings: HashMap<String, String>,
    wdir: PathBuf,
}

fn get_text_arg(node: &KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .filter(|arg| arg.name().is_none())
        .find_map(|arg| arg.value().as_string())
}

impl NodeType {
    fn infer(node: &KdlNode) -> miette::Result<NodeType> {
        Ok(match node.name().value() {
            "-" => NodeType::Text,
            "let" => NodeType::BindVar,
            // "markdown" => NodeType::InlineMarkdown,
            "@include" => NodeType::Include,
            "!doctype" => NodeType::Doctype,
            _ => NodeType::Html5,
        })
    }
}

struct Indent(usize);
impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.0 {
            f.write_char('\t')?;
        }
        Ok(())
    }
}
