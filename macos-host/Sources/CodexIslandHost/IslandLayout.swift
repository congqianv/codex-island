import Foundation

public enum ExpandedView: String {
    case list
    case detail
    case empty
}

public struct IslandLayoutMetrics {
    public let width: CGFloat
    public let height: CGFloat
    public let acceptsPointerEvents: Bool
}

private let collapsedWidth: CGFloat = 420
private let expandedWidth: CGFloat = 520
private let collapsedHeight: CGFloat = 88
private let listBaseHeight: CGFloat = 108
private let listRowHeight: CGFloat = 72
private let listRowGap: CGFloat = 12
private let listMaxHeight: CGFloat = 360
private let detailHeight: CGFloat = 300
private let emptyHeight: CGFloat = 280

public func islandLayoutMetrics(
    expanded: Bool,
    expandedView: ExpandedView,
    sessionCount: Int
) -> IslandLayoutMetrics {
    if !expanded {
        return IslandLayoutMetrics(
            width: collapsedWidth,
            height: collapsedHeight,
            acceptsPointerEvents: true
        )
    }

    let height: CGFloat
    switch expandedView {
    case .detail:
        height = detailHeight
    case .empty:
        height = emptyHeight
    case .list:
        let rows = max(sessionCount, 1)
        let gaps = max(rows - 1, 0)
        height = min(
            listBaseHeight + (CGFloat(rows) * listRowHeight) + (CGFloat(gaps) * listRowGap),
            listMaxHeight
        )
    }

    return IslandLayoutMetrics(
        width: expandedWidth,
        height: height,
        acceptsPointerEvents: true
    )
}
