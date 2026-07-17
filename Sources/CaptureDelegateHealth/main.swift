import CaptureDelegateIPC
import Foundation

@main
struct CaptureDelegateHealth {
    static func main() {
        guard CommandLine.arguments.count == 3, CommandLine.arguments[1] == "--socket" else {
            fail("usage: capture-delegate-health --socket <path>")
        }

        do {
            guard try IPCClient.checkHealth(socketPath: CommandLine.arguments[2]) else {
                fail("backend health check failed")
            }
            print("backend ok")
        } catch {
            fail("backend health check failed: \(error)")
        }
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        Foundation.exit(1)
    }
}
