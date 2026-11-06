pub trait Port {
    fn quiesce(&mut self) -> anyhow::Result<()>;
    fn backup(&mut self) -> anyhow::Result<()>;
    fn stop(&mut self) -> anyhow::Result<()>;
    fn candidate(&mut self) -> anyhow::Result<()>;
    fn health(&mut self) -> anyhow::Result<()>;
    fn commit(&mut self) -> anyhow::Result<()>;
    fn restore(&mut self) -> anyhow::Result<()>;
    fn previous(&mut self) -> anyhow::Result<()>;
    fn resume(&mut self) -> anyhow::Result<()>;
}
pub fn with_recovery<T>(
    result: anyhow::Result<T>,
    recovery: anyhow::Result<()>,
) -> anyhow::Result<T> {
    match (result, recovery) {
        (Err(primary), Err(recovery)) => {
            Err(primary.context(format!("recovery also failed: {recovery:#}")))
        }
        (Err(primary), _) => Err(primary),
        (_, Err(recovery)) => Err(recovery),
        (Ok(value), Ok(())) => Ok(value),
    }
}
pub fn replace(port: &mut impl Port) -> anyhow::Result<phone_core::update::Phase> {
    use phone_core::update::{Action, Phase};
    let mut phase = Phase::ServingIdle;
    port.quiesce()?;
    phase = phase
        .apply(Action::Quiesce)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    if let Err(error) = port.backup() {
        return with_recovery(Err(error.context("backup failed")), port.resume());
    }
    let switched = (|| -> anyhow::Result<()> {
        port.stop()?;
        phase = phase
            .apply(Action::Stop)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        port.candidate()?;
        phase = phase
            .apply(Action::StartCandidate)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        port.health()?;
        phase = phase
            .apply(Action::HealthSucceeded)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let committed = phase
            .apply(Action::Commit)
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        port.commit()?;
        phase = committed;
        Ok(())
    })();
    if let Err(error) = switched {
        let recovery = (|| -> anyhow::Result<()> {
            port.stop()
                .map_err(|error| error.context("recovery cannot stop the candidate"))?;
            port.restore()
                .map_err(|error| error.context("recovery cannot restore the backup"))?;
            port.previous().map_err(|error| {
                error.context("recovery cannot start the previous installation")
            })?;
            port.health().map_err(|error| {
                error.context("the previous installation did not pass its health check")
            })?;
            phase = phase
                .apply(Action::Rollback)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            port.resume()?;
            Ok(())
        })();
        return with_recovery(
            Err(error.context(format!("candidate failed; recovery reached {phase:?}"))),
            recovery,
        );
    }
    port.resume()
        .map_err(|error| error.context("candidate committed; service resumption can be retried"))?;
    Ok(phase)
}
