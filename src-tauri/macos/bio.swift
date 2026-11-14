// macOS 生物辨識解鎖：以 Secure Enclave P-256 金鑰（biometryCurrentSet）包裝主碼。
// 回傳碼：0 ok、1 cancel、2 lockout、3 missing、4 invalidated、5 error。
import CryptoKit
import Foundation
import LocalAuthentication
import Security

private struct Entry: Codable {
  var key: Data
  var eph: Data
  var box: Data
  var state: Data?
}

private let policy = LAPolicy.deviceOwnerAuthenticationWithBiometrics
private let info = Data("ArgoVault-bio-v1".utf8)

private func setMsg(_ s: String, _ msg: UnsafeMutablePointer<CChar>?, _ cap: Int) {
  guard let msg, cap > 0 else { return }
  let b = Array(s.utf8.prefix(cap - 1))
  for (i, c) in b.enumerated() { msg[i] = CChar(bitPattern: c) }
  msg[b.count] = 0
}

private func laCode(_ e: Error) -> Int32 {
  guard let la = e as? LAError else { return 5 }
  switch la.code {
  case .userCancel, .appCancel, .systemCancel, .userFallback: return 1
  case .biometryLockout: return 2
  case .biometryNotEnrolled: return 4
  default: return 5
  }
}

private func domainState(_ ctx: LAContext) -> Data? {
  var e: NSError?
  guard ctx.canEvaluatePolicy(policy, error: &e) else { return nil }
  if #available(macOS 15.0, *) { return ctx.domainState.biometry.stateHash }
  return ctx.evaluatedPolicyDomainState
}

private func makeContext(_ cancel: String) -> LAContext {
  let ctx = LAContext()
  ctx.localizedCancelTitle = cancel
  ctx.localizedFallbackTitle = ""
  return ctx
}

private func evaluate(_ ctx: LAContext, _ reason: String) -> Error? {
  let sem = DispatchSemaphore(value: 0)
  var err: Error?
  ctx.evaluatePolicy(policy, localizedReason: reason) { ok, e in
    if !ok { err = e ?? LAError(.authenticationFailed) }
    sem.signal()
  }
  sem.wait()
  return err
}

private func wrapKey(_ s: SharedSecret) -> SymmetricKey {
  s.hkdfDerivedSymmetricKey(using: SHA256.self, salt: Data(), sharedInfo: info, outputByteCount: 32)
}

@_cdecl("av_bio_status")
public func avBioStatus() -> Int32 {
  guard SecureEnclave.isAvailable else { return 2 }
  var e: NSError?
  if LAContext().canEvaluatePolicy(policy, error: &e) { return 0 }
  switch LAError.Code(rawValue: e?.code ?? 0) {
  case .biometryLockout: return 0
  case .biometryNotEnrolled: return 1
  case .biometryNotAvailable: return 2
  default: return 3
  }
}

@_cdecl("av_bio_enroll")
public func avBioEnroll(
  _ path: UnsafePointer<CChar>, _ secret: UnsafePointer<UInt8>, _ len: Int,
  _ reason: UnsafePointer<CChar>, _ cancel: UnsafePointer<CChar>,
  _ msg: UnsafeMutablePointer<CChar>?, _ msgCap: Int
) -> Int32 {
  let url = URL(fileURLWithPath: String(cString: path))
  let ctx = makeContext(String(cString: cancel))
  let state = domainState(ctx)
  if let e = evaluate(ctx, String(cString: reason)) {
    setMsg(e.localizedDescription, msg, msgCap)
    return laCode(e)
  }
  do {
    var cf: Unmanaged<CFError>?
    guard let ac = SecAccessControlCreateWithFlags(
      nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, [.privateKeyUsage, .biometryCurrentSet], &cf)
    else { throw cf!.takeRetainedValue() as Error }
    let se = try SecureEnclave.P256.KeyAgreement.PrivateKey(accessControl: ac, authenticationContext: ctx)
    let eph = P256.KeyAgreement.PrivateKey()
    var plain = Data(bytes: secret, count: len)
    defer { plain.resetBytes(in: 0..<plain.count) }
    let box = try AES.GCM.seal(plain, using: wrapKey(try eph.sharedSecretFromKeyAgreement(with: se.publicKey))).combined!
    var back = try AES.GCM.open(
      try AES.GCM.SealedBox(combined: box),
      using: wrapKey(try se.sharedSecretFromKeyAgreement(with: eph.publicKey)))
    defer { back.resetBytes(in: 0..<back.count) }
    guard back == plain else { throw CocoaError(.coderInvalidValue) }
    let entry = Entry(key: se.dataRepresentation, eph: eph.publicKey.rawRepresentation, box: box, state: state)
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try JSONEncoder().encode(entry).write(to: url, options: .atomic)
    try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    return 0
  } catch {
    try? FileManager.default.removeItem(at: url)
    setMsg(error.localizedDescription, msg, msgCap)
    return laCode(error)
  }
}

@_cdecl("av_bio_unlock")
public func avBioUnlock(
  _ path: UnsafePointer<CChar>, _ reason: UnsafePointer<CChar>, _ cancel: UnsafePointer<CChar>,
  _ out: UnsafeMutablePointer<UInt8>, _ outCap: Int, _ outLen: UnsafeMutablePointer<Int>,
  _ msg: UnsafeMutablePointer<CChar>?, _ msgCap: Int
) -> Int32 {
  let url = URL(fileURLWithPath: String(cString: path))
  guard let raw = try? Data(contentsOf: url), let entry = try? JSONDecoder().decode(Entry.self, from: raw)
  else { return 3 }
  let ctx = makeContext(String(cString: cancel))
  if let now = domainState(ctx), let old = entry.state, now != old { return 4 }
  if let e = evaluate(ctx, String(cString: reason)) {
    setMsg(e.localizedDescription, msg, msgCap)
    return laCode(e)
  }
  do {
    let se = try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: entry.key, authenticationContext: ctx)
    let eph = try P256.KeyAgreement.PublicKey(rawRepresentation: entry.eph)
    var plain = try AES.GCM.open(
      try AES.GCM.SealedBox(combined: entry.box),
      using: wrapKey(try se.sharedSecretFromKeyAgreement(with: eph)))
    defer { plain.resetBytes(in: 0..<plain.count) }
    guard plain.count <= outCap else { throw CocoaError(.coderInvalidValue) }
    plain.copyBytes(to: out, count: plain.count)
    outLen.pointee = plain.count
    return 0
  } catch {
    setMsg(error.localizedDescription, msg, msgCap)
    return laCode(error)
  }
}
