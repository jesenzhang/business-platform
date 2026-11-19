//! Notification bounded context — **placeholder, not implemented**.
//!
//! Wave 4 measured this crate against the notification reference set (Novu
//! v3.19.0 plus provider contracts) and recorded the real status instead of the
//! advertised one. This file used to describe in-app messages, e-mail, SMS and
//! Webhook delivery as if they existed; none of them do:
//!
//! * no `NotificationIntent`, recipient resolution, preference or opt-out model;
//! * no template versioning, idempotency, retry or failure classification;
//! * no provider adapter and no provider SDK dependency;
//! * no delivery-status model, therefore no distinction between *request
//!   accepted*, *queued*, *provider accepted*, *delivered* and *failed*;
//! * no migrations, no tests and **no consumer**: no composition root injects
//!   this crate and no other crate depends on it.
//!
//! Anything that currently needs a notification must therefore not treat a
//! successful submit as a successful delivery. The gap registry (gaps `N-G01`
//! … `N-G04`) and the acceptance preconditions live in
//! `docs/reviews/2026-09-28-saas-align-notification.md`; the
//! decision there is to keep this crate inert until a real use case schedules
//! it, rather than building a notification platform for reference alignment.
//!
//! Tracked by PLAN-0014 Wave 4 (gap `N-G01`); it is not scheduled work.
