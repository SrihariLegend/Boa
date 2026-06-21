#!/usr/bin/env node
import React from "react";
import {render} from "ink";
import {Command} from "commander";
import {App} from "./ui.js";
import {startWebServer} from "./server.js";

const program = new Command()
  .name("match-manager")
  .description("Boa engine approval match manager")
  .option("--no-alt-screen", "render in the current terminal buffer")
  .action((options: {altScreen: boolean}) => {
    render(<App />, {exitOnCtrlC: true, patchConsole: false, alternateScreen: options.altScreen});
  });

program
  .command("tui")
  .description("start the terminal UI")
  .option("--no-alt-screen", "render in the current terminal buffer")
  .action((options: {altScreen: boolean}) => {
    render(<App />, {exitOnCtrlC: true, patchConsole: false, alternateScreen: options.altScreen});
  });

program
  .command("web")
  .description("start the local web API/static server")
  .option("--host <host>", "host to bind", "127.0.0.1")
  .option("--port <port>", "port to bind", (value) => Number(value), 3777)
  .action(async (options: {host: string; port: number}) => {
    await startWebServer({host: options.host, port: options.port});
  });

program.parse();
