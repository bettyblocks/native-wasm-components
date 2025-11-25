import { getAllWasmFunctionsWithVersions } from "@betty-blocks/cli/build/functions/functionDefinitions";
import { publishWasmBlockStoreFunctions } from "@betty-blocks/cli/build/functions/publishWasmBlockStoreFunctions";
import { validateFunctions } from "@betty-blocks/cli/build/functions/validateFunctions";

import Jaws from "@betty-blocks/jaws";
import os from "node:os";
import path from "node:path";

const args = process.argv.slice(2);

const [BLOCKSTORE_CLI_SECRET, branchOrEndpoint] = args;

if (!BLOCKSTORE_CLI_SECRET) {
  throw new Error("No BLOCKSTORE_CLI_SECRET provided");
}

if (!branchOrEndpoint) {
  throw new Error("No branch provided");
}

const workingDir = process.cwd();
const baseFunctionsPath = path.join(workingDir, "functions");
const { valid } = await validateFunctions(true, baseFunctionsPath);

if (!valid) {
  process.exit(1);
}

const functionNames = getAllWasmFunctionsWithVersions(baseFunctionsPath);

const jaws = Jaws.getInstance({
  issuer: "cli",
  services: {
    cli: {
      secret: BLOCKSTORE_CLI_SECRET,
    },
  },
});

const { jwt } = jaws.sign("cli", {
  application_id: "native",
});

const authBBCli = path.join(os.homedir(), ".bb-cli.json");
await Bun.write(
  authBBCli,
  JSON.stringify(
    {
      applicationMap: {},
      auth: {
        "jwt.cli": jwt,
      },
    },
    null,
    2
  )
);

let blockstoreApiUrl =
  "https://my.bettyblocks.com/block-store-api/internal/cli";

if (branchOrEndpoint.startsWith("http")) {
  blockstoreApiUrl = blockstoreApiUrl.replace(
    "https://my.bettyblocks.com",
    branchOrEndpoint
  );
} else if (branchOrEndpoint === "edge" || branchOrEndpoint === "acceptance") {
  blockstoreApiUrl = blockstoreApiUrl.replace(
    "my.bettyblocks.com",
    `my.${branchOrEndpoint}.bettyblocks.com`
  );
}

const config = await Bun.file(path.join(workingDir, "config.json")).json();
await Bun.write(
  path.join(workingDir, "config.json"),
  JSON.stringify(
    {
      ...config,
      blockstoreApiUrl,
    },
    null,
    2
  )
);

await publishWasmBlockStoreFunctions(baseFunctionsPath, functionNames);
