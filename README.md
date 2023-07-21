# staticimp

## Introduction
staticimp (static imp) is a rust-based web service that receives user-generated content and uploads it to a GitLab (and soon GitHub) repository.
The main goal of staticimp is to support dynamic content (e.g. blog post comments) on a fully static website using automatic build+deployment.

staticimp consists of a small web service which handles POST requests from HTML forms (it can also accept json and yaml),
performs some validation and transformations, then uploads them using the GitLab REST API. staticman also supports moderation,
where the file is commited to a new branch and a merge request is created, instead of commiting the files directly to the main branch.

The actual rendering of the comments (or whatever you are using staticimp for) is up to your static site generator (SSG),
staticimp is just concerned with pushing the user generated content to the repo.

staticimp, like Staticman, creates yaml/json files in a repository, so if for example you are using a hugo theme that already supports staticman,
you should be able to support staticimp with minimal changes (staticimp.yml and changing the POST url).

## Inspiration
staticimp was inspired by the awesome project [Staticman](https://github.com/eduardoboucas/staticman).
If you already use Node.js and/or have a webserver with plenty of RAM, Staticman is a great tool and you should check it out.

While staticman is great, the memory consumption is a little heavy for running in stateless/serverless environments (e.g. [Google Cloud Run](https://cloud.google.com/run)) or on a tiny VPS.
In my use and testing it takes around 70-90+MB RAM idling, and >120MB during startup (which takes a second or two), which makes it a little tight for a VPS with 128MB total.
While that could probably be reduced some, Node isn't known for for being lightweight on RAM usage.

staticimp is my solution to the static-site-dynamic-content problem. RAM usage is not as low as it could be, but its a lot lower than staticman and it starts in milliseconds.
It doesn't (yet) support all the features of staticman, but I welcome pull requests and have a few more features planned (like GitHub support).


# Features:
- clean implementation intended to be flexible and extensible
- configuration can use placeholders to fill in/transform entries
  - uses nanotemplate (in this crate) for rendering placeholders
- loads configuration from `staticman.yml`
  - doesn't yet support project-specific config or json
- entries are validated by checking for allowed/required fields
  - doesn't yet support any formatting validation
- extra fields generated from config
- has code to load/handle field transformations (but doesn't have any implemented yet)
- can send processed entries to gitlab/debug backends
  - doesn't yet support moderated comments (commit to new branch and submit pull request),
    but I am working on that next so I can start using it on some sites I am helping build


# Work In Progress

staticimp is a work-in-progress. The features above all work, but thorough test code is still
needed and there are some missing important features that I am still implementing.
I am hoping to have staticimp feature-complete in the next week or two.

**Features still to implement**
- review branches
- thorough test code
- create and cache clients per-thread (rather than creating a new client for each request)
- load project/branch-specific config files
  - right now just loads the global conf at startup
- implement field transformations
- more documentation
- logging
- spam protection (probably reCAPTCHA)
- github as a second backend
- I might include a filesystem backend for easier configuration
- specify allowed hosts for a backend


## Requirements
- authentication token for the GitLab instance hosting your content repo
- rust
  - I plan on mostly running this in docker, so it will also include a Dockerfile + docker compose soon for easy running/tests


## Configuring Repository
The easiest way to use staticimp is have it commit files directly to your website content repository.
Your CI/CD job stays exactly the same, and you just need to configure your SSG to display the staticimp content.

Steps to get comment live (with comments in website repo):
- HTML Form POSTs to staticimp
- staticimp commits files to website repo
- CI/CD pipeline automatically rebuild site with new comments and deploy to live site

The downside to the above approach is that your git history gets cluttered with staticimp commits, and if there is lots of traffic
you can get stuck in a loop of push, fail to push because your local repo is out of data, pull, repeat.
You can work in another branch and then use GitLab merge requests to merge the changes into your main branch,
but in the best case your git history will still be thoroughly cluttered.

Since I like keeping a (relatively) clean git history, I have staticimp push files to a separate repo, and then have the main content repo pull the latest
changes on build.

Steps to get comment live (with separate comments repo):
- HTML Form POSTs to staticimp
- staticimp commits files to separate comments repo
- comments repo CI/CD triggers website repo build+deploy pipeline
- CI/CD pipeline automatically rebuild site with new comments and deploy to live site
  - added to the build step above is cloning the comments repo or making it a git submodule

## Examples

I'll write up some config examples after the features stabilize (hopefully over the next few weeks)

## Links

- [Staticman](https://github.com/eduardoboucas/staticman)
