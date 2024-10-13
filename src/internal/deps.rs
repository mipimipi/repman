use std::{collections::HashMap, fmt};

pub struct DepPkgs<'a>(Vec<&'a str>);

impl fmt::Display for DepPkgs<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = "".to_string();
        for (i, pkg_name) in self.0.iter().enumerate() {
            if i == 0 {
                s = s + pkg_name;
            } else {
                s = format!("{s}, {pkg_name}");
            }
        }
        write!(f, "{s}")
    }
}

pub struct Deps<'a>(HashMap<&'a str, DepPkgs<'a>>);

impl<'a> Deps<'a> {
    pub fn new(pkgs: &repodb_parser::Pkgs) -> anyhow::Result<Deps> {
        let mut deps = Deps(HashMap::new());

        for pkg in pkgs.packages() {
            for dep in pkg
                .deps
                .iter()
                .chain(pkg.make_deps.iter())
                .chain(pkg.check_deps.iter())
                .collect::<Vec<&repodb_parser::dep::Dep>>()
            {
                if deps.0.contains_key(dep.pkg_name.as_str()) {
                    deps.0
                        .get_mut(dep.pkg_name.as_str())
                        .unwrap()
                        .0
                        .push(&pkg.name);
                } else {
                    deps.0.insert(&dep.pkg_name, DepPkgs(vec![&pkg.name]));
                };
            }
        }

        Ok(deps)
    }

    pub fn contains_key(&self, dep: &str) -> bool {
        self.0.contains_key(dep)
    }

    pub fn get(&self, pkg_name: &str) -> Option<&DepPkgs> {
        self.0.get(pkg_name)
    }
}
