use crate::{config::AppConfig, establish_connection, run_migrations, AnyConnection};

pub struct Vacuum {
    config: AppConfig,
}

impl Vacuum {
    #[must_use]
    pub fn new(config: &AppConfig) -> Self {
        Vacuum {
            config: config.clone(),
        }
    }

    pub fn run(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let mut conn = establish_connection(&self.config.db_dsn);
        let num_deleted = self.vacuum(&mut conn)?;

        match run_migrations(&mut conn) {
            Ok(()) => {}
            Err(e) => eprintln!("Error running migrations: {e}"),
        }

        Ok(num_deleted)
    }

    fn vacuum(&self, conn: &mut AnyConnection) -> Result<usize, Box<dyn std::error::Error>> {
        let num_deleted = crate::vacuum(self.config.purge_after_days, conn)?;

        Ok(num_deleted)
    }
}
