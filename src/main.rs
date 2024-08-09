use std::error::Error;
use std::fmt::Display;

use async_graphql::{
    ComplexObject, Context, EmptySubscription, InputObject, Object, Schema, SimpleObject,
};
use numtracker::db_service::{ScanTemplates, SqliteScanPathService};
use numtracker::{paths, BeamlineContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let serv = SqliteScanPathService::connect("sqlite://./demo.db")
        .await
        .unwrap();
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .data(serv)
        .finish();
    let res = schema
        .execute(
            r#"
            mutation {
                scan(beamline: "i22", visit: {code: "cm", prop: 1234, session: 2}, sub: "foo/bar") {
                    directory
                    beamline
                    visit {
                        name
                    }
                    scanFile
                    scanNumber
                    detectors(names: ["one", "two"]) {
                        name
                        path
                    }
                }
            }"#,
        )
        .await;
    println!("{:#?}", res.data);
    let res = schema
        .execute(
            r#"
            {
                paths(beamline: "i22", visit: {code: "cm", prop: 1234, session: 2}) {
                    directory
                    beamline
                    visit {
                        name
                    }
                }
            }"#,
        )
        .await;
    println!("{:#?}", res.data);

    Ok(())
}

struct Query;

struct Mutation;

#[derive(SimpleObject)]
struct DetectorPath {
    name: String,
    path: String,
}

#[derive(SimpleObject)]
#[graphql(complex)]
struct VisitPath {
    beamline: String,
    visit: Visit,
}

#[derive(Debug, InputObject, SimpleObject)]
#[graphql(complex, input_name = "VisitInput")]
struct Visit {
    code: String,
    prop: usize,
    session: usize,
}

struct ScanPaths {
    visit: VisitPath,
    number: usize,
    subdirectory: Option<String>,
}

#[ComplexObject]
impl VisitPath {
    async fn directory(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        println!("directory");
        let db = ctx.data::<SqliteScanPathService>()?;
        let temp = db.visit_template(&self.beamline).await.unwrap();
        Ok(paths::visit_path(&temp)
            .unwrap()
            .render(&BeamlineContext::new(
                &self.beamline,
                &self.visit.to_string(),
            ))
            .to_string_lossy()
            .into())
    }
}

#[ComplexObject]
impl Visit {
    async fn name(&self) -> String {
        format!("{}{}-{}", self.code, self.prop, self.session)
    }
}

impl Display for Visit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}-{}", self.code, self.prop, self.session)
    }
}

#[Object]
impl ScanPaths {
    async fn visit(&self) -> Visit {
        Visit {
            code: "cm".into(),
            prop: 12345,
            session: 3,
        }
    }
    async fn scan_file(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        Ok(format!(
            "{}/{}/{}/{}",
            self.visit.directory(ctx).await?,
            self.visit.visit,
            self.subdirectory.as_deref().unwrap_or(""),
            self.number
        ))
    }
    async fn scan_number(&self) -> usize {
        self.number
    }
    async fn beamline(&self) -> &str {
        self.visit.beamline.as_str()
    }
    async fn directory(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        self.visit.directory(ctx).await
    }
    async fn detectors(&self, names: Vec<String>) -> Vec<DetectorPath> {
        names
            .into_iter()
            .map(|name| {
                let path = name.to_uppercase();
                DetectorPath { name, path }
            })
            .collect()
    }
}

#[Object]
impl Query {
    async fn paths(&self, beamline: String, visit: Visit) -> VisitPath {
        VisitPath { beamline, visit }
    }
}

#[Object]
impl Mutation {
    async fn scan<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        beamline: String,
        visit: Visit,
        sub: Option<String>,
    ) -> async_graphql::Result<ScanPaths> {
        let db = ctx.data::<SqliteScanPathService>()?;
        let number = db.next_scan_number(&beamline).await.unwrap();
        Ok(ScanPaths {
            visit: VisitPath { beamline, visit },
            number,
            subdirectory: sub,
        })
    }
}
