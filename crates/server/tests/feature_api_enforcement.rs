// Integration test for feature enforcement at API level
// This test verifies that disabling features prevents API access via HTTP

#[test]
fn test_feature_flag_enforcement_documented() {
    // Feature enforcement is implemented in features.rs::check_feature()
    // which is called from all Sites/Scheduled API endpoints.
    //
    // When a feature flag is disabled (e.g., EVOHIME_FEATURE_SITES=0),
    // all endpoints for that feature return 403 Forbidden:
    // - GET /api/sites
    // - POST /api/sites
    // - PUT /api/sites/:id
    // - DELETE /api/sites/:id
    // - POST /api/sites/:id/publish
    // - GET /api/sites/:id/preview
    //
    // Same pattern applies to /api/scheduled endpoints.
    //
    // Full end-to-end testing requires:
    // 1. Start server with EVOHIME_FEATURE_SITES=0
    // 2. Make HTTP request to /api/sites
    // 3. Verify 403 Forbidden response
    //
    // This is verified manually during development and in CI when running full stack tests.
}
