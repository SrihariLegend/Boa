#!/usr/bin/env node
import React from "react";
import {render} from "ink";
import {Command} from "commander";
import {App} from "./ui.js";

const program = new Command()
  .name("match-manager")
  .description("Boa engine approval match manager")
  .option("--no-alt-screen", "render in the current terminal buffer")
  .action((options: {altScreen: boolean}) => {
    render(<App />, {exitOnCtrlC: true, patchConsole: false, alternateScreen: options.altScreen});
  });

program.parse();
