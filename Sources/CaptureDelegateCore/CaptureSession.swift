import Foundation

public struct CaptureSession: Identifiable, Codable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var note: String
    public let createdAt: Date
    public let duration: TimeInterval
    public let source: String

    public init(
        id: UUID,
        title: String,
        note: String,
        createdAt: Date,
        duration: TimeInterval,
        source: String
    ) {
        self.id = id
        self.title = title
        self.note = note
        self.createdAt = createdAt
        self.duration = duration
        self.source = source
    }
}
