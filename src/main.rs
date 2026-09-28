mod chaseplane_interface;
mod trackaudio_interface;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use crate::{chaseplane_interface, trackaudio_interface};

    tokio::try_join!(
        async {
            trackaudio_interface::connect()
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        },
        chaseplane_interface::run(),
    )?;

    Ok(())
}
