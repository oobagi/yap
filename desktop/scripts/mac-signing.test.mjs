import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

const require = createRequire(import.meta.url);
const builderRequire = createRequire(require.resolve("electron-builder"));
const signingPath = builderRequire.resolve("app-builder-lib/out/codeSign/macCodeSign.js");

// Exercise the installed dependency without touching the host's keychains.
// Regression: https://github.com/electron-userland/electron-builder/issues/10066
for (const certificatePassword of ["certificate-password", ""]) {
  test(`macOS signing keeps keychain and certificate passwords separate (${certificatePassword ? "protected" : "unprotected"} certificate)`, async () => {
    const commands = [];
    const signing = {};
    const mocks = {
      "builder-util": {
        exec: async (command, args) => {
          assert.equal(command, "/usr/bin/security");
          commands.push(Array.from(args));
          return "";
        },
      },
      "lazy-val": { Lazy: class {} },
      "./codesign": { importCertificate: async (link) => link },
      "@electron/osx-sign": {},
      "@electron/osx-sign/dist/cjs/util-identities": {},
      "temp-file": {},
      "../util/flags": {},
    };
    runInNewContext(readFileSync(signingPath, "utf8"), {
      exports: signing,
      process: { env: { TRAVIS: "true" } },
      require: (name) => {
        if (Object.hasOwn(mocks, name)) return mocks[name];
        assert.ok(["crypto", "fs/promises", "os", "path"].includes(name), `Unexpected dependency: ${name}`);
        return require(name);
      },
    }, { filename: signingPath });

    const result = await signing.createKeychain({
      tmpDir: {},
      currentDir: "/synthetic-yap-build",
      cscLink: "/synthetic-application.p12",
      cscKeyPassword: certificatePassword,
      cscILink: "/synthetic-installer.p12",
      cscIKeyPassword: "installer-password",
    });
    const create = commands.find(([operation]) => operation === "create-keychain");
    const keychainPassword = create[create.indexOf("-p") + 1];
    assert.ok(keychainPassword.length > 0);
    assert.notEqual(keychainPassword, certificatePassword);
    const unlock = commands.find(([operation]) => operation === "unlock-keychain");
    assert.equal(unlock[unlock.indexOf("-p") + 1], keychainPassword);

    const imports = commands.filter(([operation]) => operation === "import");
    assert.equal(imports.length, 2);
    assert.deepEqual(imports.map((args) => args[args.indexOf("-P") + 1]), [certificatePassword, "installer-password"]);
    const partitions = commands.filter(([operation]) => operation === "set-key-partition-list");
    assert.equal(partitions.length, imports.length);
    for (const args of partitions) {
      assert.equal(args[args.indexOf("-k") + 1], keychainPassword);
      assert.equal(args.at(-1), result.keychainFile);
    }
  });
}
