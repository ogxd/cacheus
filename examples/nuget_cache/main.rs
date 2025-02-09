use cacheus::{self, config2::Configuration};

#[tokio::main]
async fn main()
{
    let contents = std::fs::read_to_string("examples/nuget_cache/config.yaml")
        .expect("Could not find configuration file");

    let configuration: Configuration = serde_yaml::from_str::<Configuration>(&contents)
        .expect("Could not parse configuration file");

    // Serialize back to yaml string and print
    let serialized = serde_yaml::to_string(&configuration).unwrap();
    println!("{}", serialized);
}
