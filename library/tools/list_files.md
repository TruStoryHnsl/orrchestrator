---
name: List Files
description: Lists files and directories at a given path.
tags: [filesystem, listing]
---
<tool_definition>
  <tool_name>list_files</tool_name>
  <description>
    Lists the files and directories in a specified directory path.
    Returns a JSON-formatted list of entries.
  </description>
  <parameters>
    <parameter>
      <name>path</name>
      <type>string</type>
      <description>The path to the directory to list. Defaults to the current directory if not provided.</description>
    </parameter>
  </parameters>
</tool_definition>
