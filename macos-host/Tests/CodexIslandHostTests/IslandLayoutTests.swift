import XCTest
@testable import CodexIslandHost

final class IslandLayoutTests: XCTestCase {
    func testCollapsedMetricsMatchCapsule() {
        let metrics = islandLayoutMetrics(expanded: false, expandedView: .list, sessionCount: 2)

        XCTAssertEqual(metrics.width, 420)
        XCTAssertEqual(metrics.height, 88)
        XCTAssertTrue(metrics.acceptsPointerEvents)
    }

    func testListHeightTracksVisibleRows() {
        XCTAssertEqual(
            islandLayoutMetrics(expanded: true, expandedView: .list, sessionCount: 1).height,
            180
        )
        XCTAssertEqual(
            islandLayoutMetrics(expanded: true, expandedView: .list, sessionCount: 3).height,
            348
        )
    }

    func testDetailAndEmptyUseCompactHeights() {
        XCTAssertEqual(
            islandLayoutMetrics(expanded: true, expandedView: .detail, sessionCount: 5).height,
            300
        )
        XCTAssertEqual(
            islandLayoutMetrics(expanded: true, expandedView: .empty, sessionCount: 0).height,
            280
        )
    }
}
