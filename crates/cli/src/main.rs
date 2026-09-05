#![forbid(unsafe_code)]
use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use inferqos_config::{Config, Mode};
use inferqos_core::{
    AdmissionRequest, EstimateSource, IdentityContext, ServiceClass, WorkEstimate, WorkUnits,
};
use inferqos_proxy::AppState;
use inferqos_scheduler::{Scheduler, SchedulerConfig, VirtualClock};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name="inferqos",version,about="QoS control plane for finite AI inference capacity",long_about=None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Serve(ConfigArg),
    Init {
        #[arg(default_value = "inferqos.yaml")]
        output: PathBuf,
        #[arg(long)]
        enforce: bool,
    },
    Validate(ConfigArg),
    Doctor {
        #[command(flatten)]
        config: ConfigArg,
        #[arg(long)]
        target: Option<DeployTarget>,
    },
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },
    Capacity {
        #[command(subcommand)]
        command: CapacityCommand,
    },
    Analyze(ReplayArgs),
    Replay(ReplayArgs),
    Shadow(ConfigArg),
    Benchmark {
        #[arg(long, default_value_t = 100_000)]
        decisions: u64,
    },
    Deploy(DeployArgs),
    Upgrade {
        #[arg(long)]
        check: bool,
    },
    Version,
    Explain {
        request_id: Uuid,
        #[arg(long, default_value = "http://127.0.0.1:9090")]
        admin: String,
    },
    Diagnostics {
        #[command(subcommand)]
        command: DiagnosticsCommand,
    },
}
#[derive(Subcommand)]
enum PolicyCommand {
    Test {
        #[command(flatten)]
        config: ConfigArg,
        #[arg(long)]
        application: String,
        #[arg(long)]
        tenant: String,
        #[arg(long)]
        class: ServiceClass,
    },
}
#[derive(Subcommand)]
enum CapacityCommand {
    Status {
        #[arg(long, default_value = "http://127.0.0.1:9090")]
        admin: String,
    },
}
#[derive(Subcommand)]
enum DiagnosticsCommand {
    Collect {
        #[command(flatten)]
        config: ConfigArg,
        #[arg(long, default_value = "inferqos-diagnostics.json")]
        output: PathBuf,
    },
}
#[derive(Args, Clone)]
struct ConfigArg {
    #[arg(short, long, default_value = "inferqos.yaml", env = "INFERQOS_CONFIG")]
    config: PathBuf,
}
#[derive(Args)]
struct ReplayArgs {
    input: PathBuf,
    #[arg(long)]
    capacity: f64,
    #[arg(long)]
    cost_per_capacity_unit: Option<f64>,
    #[arg(long)]
    capacity_increment: Option<f64>,
    #[arg(long)]
    json: Option<PathBuf>,
    #[arg(long)]
    html: Option<PathBuf>,
}
#[derive(Args)]
struct DeployArgs {
    #[arg(long, value_enum, default_value = "auto")]
    target: DeployTarget,
    #[arg(long)]
    dry_run: bool,
    #[arg(short, long, default_value = "inferqos.yaml")]
    config: PathBuf,
    #[arg(long, default_value = "ghcr.io/dlamaro96/inferqos:latest")]
    image: String,
    #[arg(long, default_value = "inferqos")]
    name: String,
    #[arg(long)]
    resource_group: Option<String>,
    #[arg(long)]
    region: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    vpc_id: Option<String>,
    #[arg(long, value_delimiter = ',')]
    subnets: Vec<String>,
    #[arg(long)]
    config_secret: Option<String>,
}
#[derive(Clone, Copy, ValueEnum)]
enum DeployTarget {
    Auto,
    Docker,
    Aca,
    Kubernetes,
    Ecs,
    CloudRun,
    Systemd,
}

#[tokio::main]
async fn main() -> Result<()> {
    let telemetry = inferqos_telemetry::init()?;
    let result = match Cli::parse().command {
        Commands::Serve(a) => serve(a.config, None).await,
        Commands::Shadow(a) => serve(a.config, Some(Mode::Shadow)).await,
        Commands::Init { output, enforce } => init(&output, enforce),
        Commands::Validate(a) => validate(&a.config),
        Commands::Doctor { config, target } => doctor(&config.config, target).await,
        Commands::Policy { command } => policy(command),
        Commands::Capacity { command } => capacity(command).await,
        Commands::Analyze(a) | Commands::Replay(a) => replay(a),
        Commands::Benchmark { decisions } => benchmark(decisions),
        Commands::Deploy(args) => deploy(args),
        Commands::Upgrade { check } => upgrade(check).await,
        Commands::Version => {
            version();
            Ok(())
        }
        Commands::Explain { request_id, admin } => explain(request_id, &admin).await,
        Commands::Diagnostics { command } => diagnostics(command).await,
    };
    telemetry.shutdown();
    result
}

async fn serve(path: PathBuf, mode: Option<Mode>) -> Result<()> {
    let mut config = Config::from_path(&path)?;
    if let Some(mode) = mode {
        config.mode = mode
    }
    let data = config.server.listen;
    let admin = config.admin.listen;
    let app = AppState::build(config.clone()).await?;
    app.start_config_reload(path.clone(), config.server.config_reload_interval);
    let admin_listener = tokio::net::TcpListener::bind(admin)
        .await
        .with_context(|| format!("cannot bind admin at {admin}"))?;
    println!(
        "InferQoS ready\n\nProxy:      http://{data}\nAdmin:      http://{admin}\nDashboard:  http://{admin}/ui\nMode:       {:?}\nPools:      {} configured\n\nNext:\n  inferqos doctor\n  inferqos capacity status",
        config.mode,
        config.pools.len()
    );
    let data_app = app.data_router();
    let admin_app = app.admin_router();
    let admin_server = axum::serve(admin_listener, admin_app);
    if let Some(tls) = &config.server.tls {
        let rustls = load_mtls_config(tls).await?;
        let acceptor = MtlsAcceptor::new(axum_server::tls_rustls::RustlsAcceptor::new(rustls));
        let data_server = axum_server::bind(data)
            .acceptor(acceptor)
            .serve(data_app.into_make_service_with_connect_info::<std::net::SocketAddr>());
        tokio::select! {r=data_server=>r.context("mTLS proxy server failed")?,r=admin_server=>r.context("admin server failed")?,_=shutdown_signal()=>{}}
    } else {
        let data_listener = tokio::net::TcpListener::bind(data)
            .await
            .with_context(|| format!("cannot bind proxy at {data}"))?;
        let data_server = axum::serve(
            data_listener,
            data_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        );
        tokio::select! {r=data_server=>r.context("proxy server failed")?,r=admin_server=>r.context("admin server failed")?,_=shutdown_signal()=>{}}
    }
    app.begin_drain();
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.active() > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler installation must succeed");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn load_mtls_config(
    tls: &inferqos_config::ServerTlsConfig,
) -> Result<axum_server::tls_rustls::RustlsConfig> {
    use rustls::{
        RootCertStore,
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
        server::WebPkiClientVerifier,
    };

    let cert_pem = tokio::fs::read(&tls.cert_file).await.with_context(|| {
        format!(
            "cannot read server TLS certificate {}",
            tls.cert_file.display()
        )
    })?;
    let key_pem = tokio::fs::read(&tls.key_file).await.with_context(|| {
        format!(
            "cannot read server TLS private key {}",
            tls.key_file.display()
        )
    })?;
    let ca_pem = tokio::fs::read(&tls.client_ca_file)
        .await
        .with_context(|| {
            format!(
                "cannot read mTLS client CA {}",
                tls.client_ca_file.display()
            )
        })?;
    let certs = CertificateDer::pem_slice_iter(&cert_pem).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&key_pem)
        .context("server TLS private key file contains no supported private key")?;
    let mut roots = RootCertStore::empty();
    let client_roots = CertificateDer::pem_slice_iter(&ca_pem).collect::<Result<Vec<_>, _>>()?;
    let (added, ignored) = roots.add_parsable_certificates(client_roots);
    if added == 0 || ignored > 0 {
        bail!("mTLS client CA contains no usable certificates or contains malformed certificates");
    }
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .context("cannot build mTLS client verifier")?;
    let server = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .context("server TLS certificate and private key do not match")?;
    Ok(axum_server::tls_rustls::RustlsConfig::from_config(
        Arc::new(server),
    ))
}
fn init(path: &Path, enforce: bool) -> Result<()> {
    if path.exists() {
        bail!(
            "{} already exists; refusing to overwrite it",
            path.display()
        )
    }
    let mode = if enforce { "enforce" } else { "shadow" };
    std::fs::write(path, EXAMPLE.replace("MODE", mode))
        .with_context(|| format!("cannot write {}", path.display()))?;
    println!(
        "Wrote {} in {mode} mode. Set INFERQOS_UPSTREAM, then run:\n  inferqos validate --config {}\n  inferqos serve --config {}",
        path.display(),
        path.display(),
        path.display()
    );
    Ok(())
}
fn validate(path: &Path) -> Result<()> {
    let config = Config::from_path(path)?;
    config.validate()?;
    println!(
        "{} is valid ({}, {} pool(s), {} application policies)",
        path.display(),
        config.api_version,
        config.pools.len(),
        config.policies.applications.len()
    );
    Ok(())
}
async fn doctor(path: &Path, target: Option<DeployTarget>) -> Result<()> {
    let config = Config::from_path(path)?;
    println!("✓ configuration is valid");
    for (name, pool) in &config.pools {
        let parsed = reqwest::Url::parse(&pool.endpoint)
            .with_context(|| format!("pool {name} has invalid endpoint"))?;
        let host = parsed.host_str().context("endpoint has no host")?;
        let mut addresses =
            tokio::net::lookup_host((host, parsed.port_or_known_default().unwrap_or(443)))
                .await
                .with_context(|| format!("DNS failed for pool {name} host {host}"))?;
        if addresses.next().is_none() {
            bail!("DNS returned no addresses for pool {name} host {host}");
        }
        println!("✓ pool {name}: DNS and endpoint syntax valid");
    }
    if let Some(t) = target {
        let tool = match t {
            DeployTarget::Docker => "docker",
            DeployTarget::Aca => "az",
            DeployTarget::Kubernetes => "helm",
            DeployTarget::Ecs => "aws",
            DeployTarget::CloudRun => "gcloud",
            DeployTarget::Systemd => "systemctl",
            DeployTarget::Auto => "docker",
        };
        if which(tool) {
            println!("✓ deployment prerequisite found: {tool}")
        } else {
            bail!("deployment target requires '{tool}', but it is not on PATH")
        }
    }
    println!("Doctor completed without exposing credentials or sending request bodies.");
    Ok(())
}
fn policy(command: PolicyCommand) -> Result<()> {
    let PolicyCommand::Test {
        config,
        application,
        tenant,
        class,
    } = command;
    let c = Config::from_path(&config.config)?;
    let app = c
        .policies
        .applications
        .get(&application)
        .with_context(|| format!("application {application} is not configured"))?;
    let allowed = app.tenant == tenant && app.allowed_classes.contains(&class.to_string());
    println!(
        "requested_class={class}\ntenant={tenant}\napplication={application}\neffective_class={}\nreason={}",
        if allowed {
            class
        } else {
            ServiceClass::Standard
        },
        if allowed {
            "entitlement permits requested class"
        } else {
            "request is downgraded because identity or class entitlement does not match"
        }
    );
    Ok(())
}
async fn capacity(command: CapacityCommand) -> Result<()> {
    let CapacityCommand::Status { admin } = command;
    let value: reqwest::Response =
        reqwest::get(format!("{}/api/v1/capacity", admin.trim_end_matches('/')))
            .await
            .context("cannot reach InferQoS admin API")?;
    if !value.status().is_success() {
        bail!("admin API returned {}", value.status())
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&value.json::<serde_json::Value>().await?)?
    );
    Ok(())
}
fn replay(args: ReplayArgs) -> Result<()> {
    let events = inferqos_simulator::read_events(&args.input)?;
    let report = inferqos_simulator::replay(
        &events,
        args.capacity,
        args.cost_per_capacity_unit,
        args.capacity_increment,
    )?;
    print!("{}", inferqos_simulator::terminal(&report));
    if let Some(path) = args.json {
        std::fs::write(&path, serde_json::to_vec_pretty(&report)?)
            .with_context(|| format!("cannot write {}", path.display()))?
    }
    if let Some(path) = args.html {
        std::fs::write(&path, inferqos_simulator::html(&report)?)
            .with_context(|| format!("cannot write {}", path.display()))?
    }
    Ok(())
}
fn benchmark(n: u64) -> Result<()> {
    let clock = Arc::new(VirtualClock::default());
    let scheduler = Scheduler::new(clock, SchedulerConfig::default());
    let start = Instant::now();
    for i in 0..n {
        let r = AdmissionRequest {
            id: Uuid::new_v4(),
            identity: IdentityContext {
                principal: "bench".into(),
                tenant: format!("tenant-{}", i % 100),
                application: "bench".into(),
                trusted: true,
            },
            requested_class: ServiceClass::Standard,
            effective_class: ServiceClass::Standard,
            pool: "primary".into(),
            estimate: WorkEstimate {
                input_tokens: 100,
                output_tokens: 100,
                cached_input_tokens: 0,
                provider_cost_coefficient: 1.0,
                normalized_units: WorkUnits(200.0),
                confidence: 1.0,
                source: EstimateSource::ExactTokenizer,
            },
            deadline: Duration::from_secs(10),
            queueable: true,
        };
        scheduler.enqueue(r, 0, 20, 1, 1)?;
        scheduler.pop_next();
    }
    let elapsed = start.elapsed();
    println!(
        "{n} scheduler decisions in {elapsed:?}; mean {:?}",
        elapsed / n as u32
    );
    Ok(())
}
fn deploy(options: DeployArgs) -> Result<()> {
    Config::from_path(&options.config).context("deployment configuration is invalid")?;
    let target = match options.target {
        DeployTarget::Auto if which("docker") => DeployTarget::Docker,
        DeployTarget::Auto => bail!(
            "auto detection found no safe target; install Docker or select --target explicitly"
        ),
        x => x,
    };
    let required = |value: &Option<String>, flag: &str| {
        value
            .clone()
            .with_context(|| format!("{} deployment requires {flag}", target_name(target)))
    };
    let config = options.config.canonicalize().with_context(|| {
        format!(
            "cannot resolve deployment config {}",
            options.config.display()
        )
    })?;
    let (image_repository, image_tag) = options
        .image
        .rsplit_once(':')
        .map_or((options.image.as_str(), "latest"), |(repository, tag)| {
            (repository, tag)
        });
    let (cmd, args): (String, Vec<String>) = match target {
        DeployTarget::Docker => (
            "docker".into(),
            vec![
                "run".into(),
                "-d".into(),
                "--name".into(),
                options.name.clone(),
                "--restart".into(),
                "unless-stopped".into(),
                "--read-only".into(),
                "--security-opt".into(),
                "no-new-privileges:true".into(),
                "--tmpfs".into(),
                "/tmp:rw,noexec,nosuid,size=64m".into(),
                "-p".into(),
                "8080:8080".into(),
                "-p".into(),
                "127.0.0.1:9090:9090".into(),
                "-v".into(),
                format!("{}:/etc/inferqos/inferqos.yaml:ro", config.display()),
                options.image.clone(),
                "serve".into(),
                "--config".into(),
                "/etc/inferqos/inferqos.yaml".into(),
            ],
        ),
        DeployTarget::Kubernetes => (
            "helm".into(),
            vec![
                "upgrade".into(),
                "--install".into(),
                "inferqos".into(),
                "deploy/kubernetes/helm".into(),
                "--namespace".into(),
                "inferqos".into(),
                "--create-namespace".into(),
                "--set-string".into(),
                format!("image.repository={image_repository},image.tag={image_tag}"),
                "--set-file".into(),
                format!("config={}", config.display()),
            ],
        ),
        DeployTarget::Aca => (
            "az".into(),
            vec![
                "deployment".into(),
                "group".into(),
                "create".into(),
                "--resource-group".into(),
                required(&options.resource_group, "--resource-group")?,
                "--name".into(),
                options.name.clone(),
                "--template-file".into(),
                "deploy/azure/container-apps/main.bicep".into(),
                "--parameters".into(),
                format!("configYaml=@{}", config.display()),
                format!("image={}", options.image),
            ],
        ),
        DeployTarget::Ecs => (
            "aws".into(),
            vec![
                "cloudformation".into(),
                "deploy".into(),
                "--template-file".into(),
                "deploy/aws/ecs/template.yaml".into(),
                "--stack-name".into(),
                options.name.clone(),
                "--capabilities".into(),
                "CAPABILITY_NAMED_IAM".into(),
                "--region".into(),
                required(&options.region, "--region")?,
                "--parameter-overrides".into(),
                format!("Image={}", options.image),
                format!("VpcId={}", required(&options.vpc_id, "--vpc-id")?),
                format!(
                    "Subnets={}",
                    if options.subnets.is_empty() {
                        bail!("ecs deployment requires --subnets subnet-a,subnet-b")
                    } else {
                        options.subnets.join(",")
                    }
                ),
                format!(
                    "ConfigSecretArn={}",
                    required(&options.config_secret, "--config-secret")?
                ),
            ],
        ),
        DeployTarget::CloudRun => (
            "gcloud".into(),
            vec![
                "run".into(),
                "deploy".into(),
                options.name.clone(),
                "--image".into(),
                options.image.clone(),
                "--project".into(),
                required(&options.project, "--project")?,
                "--region".into(),
                required(&options.region, "--region")?,
                "--ingress".into(),
                "internal".into(),
                "--no-allow-unauthenticated".into(),
                "--min".into(),
                "2".into(),
                "--max".into(),
                "10".into(),
                "--concurrency".into(),
                "50".into(),
                "--port".into(),
                "8080".into(),
                "--set-secrets".into(),
                format!(
                    "INFERQOS_CONFIG_YAML={}:latest",
                    required(&options.config_secret, "--config-secret")?
                ),
                "--args".into(),
                "serve,--config,/etc/inferqos/runtime.yaml".into(),
            ],
        ),
        DeployTarget::Systemd => (
            "sudo".into(),
            vec![
                "bash".into(),
                "deploy/systemd/install.sh".into(),
                std::env::current_exe()
                    .context("cannot locate the running InferQoS binary")?
                    .display()
                    .to_string(),
                config.display().to_string(),
            ],
        ),
        DeployTarget::Auto => unreachable!(),
    };
    println!("Will run: {cmd} {}", args.join(" "));
    if options.dry_run {
        return Ok(());
    }
    if !which(&cmd) {
        bail!("required deployment command '{cmd}' is not installed")
    }
    let auth_check: Option<(&str, &[&str])> = match target {
        DeployTarget::Docker => Some(("docker", &["info"])),
        DeployTarget::Aca => Some(("az", &["account", "show", "--output", "none"])),
        DeployTarget::Kubernetes => Some(("kubectl", &["cluster-info"])),
        DeployTarget::Ecs => Some(("aws", &["sts", "get-caller-identity", "--output", "json"])),
        DeployTarget::CloudRun => Some(("gcloud", &["auth", "print-access-token"])),
        DeployTarget::Systemd | DeployTarget::Auto => None,
    };
    if let Some((tool, args)) = auth_check {
        if !which(tool) {
            bail!("deployment target requires '{tool}', but it is not on PATH")
        }
        let status = Command::new(tool)
            .args(args)
            .status()
            .with_context(|| format!("could not execute {tool} authentication check"))?;
        if !status.success() {
            bail!(
                "{tool} authentication or target connectivity check failed; authenticate normally, then run inferqos doctor --target {}",
                target_name(target)
            )
        }
    }
    let status = Command::new(&cmd)
        .args(&args)
        .status()
        .with_context(|| format!("failed to start {cmd}"))?;
    if !status.success() {
        bail!(
            "deployment command exited with {status}; run 'inferqos doctor --target {}' for prerequisites",
            target_name(target)
        )
    }
    println!(
        "Deployment completed. Admin access remains private by default. Verify /health/ready from the target network, then point a client base URL at the data-plane endpoint."
    );
    Ok(())
}
async fn upgrade(check: bool) -> Result<()> {
    let response = reqwest::Client::new()
        .get("https://api.github.com/repos/dlamaro96/inferqos/releases/latest")
        .header("user-agent", "inferqos-upgrade")
        .send()
        .await
        .context("cannot query GitHub Releases")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("no published InferQoS release exists yet")
    };
    let json: serde_json::Value = response.error_for_status()?.json().await?;
    let tag = json["tag_name"].as_str().context("release has no tag")?;
    println!("Current: v{}\nLatest:  {tag}", env!("CARGO_PKG_VERSION"));
    if !check {
        println!(
            "Run the verified installer for {tag}; it validates SHA256 checksums before replacing the binary."
        )
    }
    Ok(())
}
fn version() {
    println!(
        "inferqos {}\ngit_sha {}\nbuild_target {}\nfeatures default",
        env!("CARGO_PKG_VERSION"),
        option_env!("INFERQOS_GIT_SHA").unwrap_or("unknown"),
        option_env!("INFERQOS_BUILD_TARGET").unwrap_or(std::env::consts::ARCH)
    )
}
async fn explain(id: Uuid, admin: &str) -> Result<()> {
    let list: reqwest::Response =
        reqwest::get(format!("{}/api/v1/decisions", admin.trim_end_matches('/'))).await?;
    let values: Vec<serde_json::Value> = list.error_for_status()?.json().await?;
    let found = values
        .iter()
        .find(|v| v["request_id"].as_str() == Some(&id.to_string()))
        .with_context(|| format!("request {id} is not in the bounded decision history"))?;
    println!("{}", serde_json::to_string_pretty(found)?);
    Ok(())
}
async fn diagnostics(command: DiagnosticsCommand) -> Result<()> {
    let DiagnosticsCommand::Collect { config, output } = command;
    let c = Config::from_path(&config.config)?;
    let sanitized = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"mode":c.mode,"server":c.server.listen.to_string(),"admin":c.admin.listen.to_string(),"pools":c.pools.iter().map(|(n,p)|serde_json::json!({"name":n,"provider":p.provider,"capacity_units":p.capacity_units,"endpoint_host":reqwest::Url::parse(&p.endpoint).ok().and_then(|u|u.host_str().map(str::to_owned))})).collect::<Vec<_>>(),"note":"Secrets, request bodies, prompts, completions, and tokens are intentionally excluded."});
    std::fs::write(&output, serde_json::to_vec_pretty(&sanitized)?)?;
    println!("Wrote sanitized diagnostic bundle to {}", output.display());
    Ok(())
}

#[derive(Clone, Debug)]
struct MtlsAcceptor<A = axum_server::accept::DefaultAcceptor> {
    inner: axum_server::tls_rustls::RustlsAcceptor<A>,
}

impl MtlsAcceptor {
    fn new(inner: axum_server::tls_rustls::RustlsAcceptor) -> Self {
        Self { inner }
    }
}

impl<I, S, A> axum_server::accept::Accept<I, S> for MtlsAcceptor<A>
where
    A: axum_server::accept::Accept<I, S> + Clone + Send + 'static,
    A::Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    A::Service: Send,
    A::Future: Send,
    I: Send + 'static,
    S: Send + 'static,
{
    type Stream = tokio_rustls::server::TlsStream<A::Stream>;
    type Service = VerifiedCertificateService<A::Service>;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::io::Result<(Self::Stream, Self::Service)>> + Send,
        >,
    >;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            use sha2::{Digest, Sha256};
            let (tls_stream, inner_service) = inner.accept(stream, service).await?;
            let (_, connection) = tls_stream.get_ref();
            let fingerprints = connection
                .peer_certificates()
                .into_iter()
                .flatten()
                .map(|certificate| {
                    Sha256::digest(certificate.as_ref())
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect()
                })
                .collect();
            let certificate = inferqos_identity::VerifiedClientCertificate {
                sha256_fingerprints: fingerprints,
            };
            Ok((
                tls_stream,
                VerifiedCertificateService {
                    inner: inner_service,
                    certificate,
                },
            ))
        })
    }
}

#[derive(Clone, Debug)]
struct VerifiedCertificateService<S> {
    inner: S,
    certificate: inferqos_identity::VerifiedClientCertificate,
}

impl<S, B> tower_service::Service<http::Request<B>> for VerifiedCertificateService<S>
where
    S: tower_service::Service<http::Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, mut request: http::Request<B>) -> Self::Future {
        request.extensions_mut().insert(self.certificate.clone());
        self.inner.call(request)
    }
}

fn which(name: &str) -> bool {
    Command::new(name).arg("--version").output().is_ok()
}
fn target_name(t: DeployTarget) -> &'static str {
    match t {
        DeployTarget::Auto => "auto",
        DeployTarget::Docker => "docker",
        DeployTarget::Aca => "aca",
        DeployTarget::Kubernetes => "kubernetes",
        DeployTarget::Ecs => "ecs",
        DeployTarget::CloudRun => "cloud-run",
        DeployTarget::Systemd => "systemd",
    }
}
const EXAMPLE: &str = r#"apiVersion: inferqos.io/v1alpha1
kind: InferQoSConfig
mode: MODE
server:
  listen: 0.0.0.0:8080
  max_body_bytes: 16777216
  spool_threshold_bytes: 262144
admin:
  listen: 127.0.0.1:9090
  expose_decisions: false
coordinator:
  type: memory
service_classes:
  realtime: { weight: 100, default_deadline: 500ms, max_queue: 100ms, max_queued: 100 }
  interactive: { weight: 50, default_deadline: 3s, max_queue: 3s, max_queued: 1000 }
  standard: { weight: 20, default_deadline: 10s, max_queue: 10s, max_queued: 3000 }
  workflow: { weight: 10, default_deadline: 30s, max_queue: 60s, max_queued: 3000 }
  batch: { weight: 1, default_deadline: 30m, max_queue: 30m, max_queued: 3000 }
pools:
  primary:
    provider: openai-compatible
    endpoint: ${INFERQOS_UPSTREAM}
    model: null
    deployment: null
    capacity_units: 50000
    auth: { type: bearer, env: INFERQOS_UPSTREAM_API_KEY }
    allowed_hosts: []
    initial_safety_factor: 1.15
policies:
  tenants:
    default: { weight: 1, guaranteed_share: 0.0, max_share: 1.0, max_concurrency: 100 }
  applications: {}
  api_keys: {}
limits:
  total_queue_depth: 10000
  total_queue_bytes: 268435456
  decision_history: 2048
  expected_replicas: 1
  allow_unsafe_uncoordinated_ha: false
"#;
