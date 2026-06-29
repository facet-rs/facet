// a repo admin can pushRepo, pullRepo, forkRepo, 
// addRepoReporter, addRepoContributor, addRepoAdmin
@id("resource.admins_Repo")
permit (
  principal,
  action in
    [GitApp::Action::"pushRepo",
     GitApp::Action::"pullRepo",
     GitApp::Action::"forkRepo",
     GitApp::Action::"addRepoReporter",
     GitApp::Action::"addRepoContributor",
     GitApp::Action::"addRepoAdmin"],
  resource in GitApp::Application::"GitApp"
)
when { principal in resource.admins };

// a repo admin can addIssue, editIssue, 
// assignIssue, closeIssue, deleteIssue
@id("resource.admins_Issue")
permit (
  principal,
  action in
    [GitApp::Action::"addIssue",
     GitApp::Action::"editIssue",
     GitApp::Action::"assignIssue",
     GitApp::Action::"closeIssue",
     GitApp::Action::"deleteIssue"],
  resource in GitApp::Application::"GitApp"
)
when { principal in resource.admins };

// repo reporter can addIssue, pullRepo, forkRepo
@id("resource.reporters")
permit (
  principal,
  action in
    [GitApp::Action::"pullRepo",
     GitApp::Action::"forkRepo",
     GitApp::Action::"addIssue"],
  resource in GitApp::Application::"GitApp"
)
when { resource has reporters && principal in resource.reporters };

// a repo contributor can addIssue, editIssue, assignIssue, closeIssue
@id("resource.contributors_Issue")
permit (
  principal,
  action in
    [GitApp::Action::"addIssue",
     GitApp::Action::"editIssue",
     GitApp::Action::"assignIssue",
     GitApp::Action::"closeIssue"],
  resource in GitApp::Application::"GitApp"
)
when { resource has contributors && principal in resource.contributors };

// a repo contributor can pushRepo, pullRepo, forkRepo
@id("resource.contributors_Repo")
permit (
  principal,
  action in
    [GitApp::Action::"pushRepo",
     GitApp::Action::"pullRepo",
     GitApp::Action::"forkRepo"],
  resource in GitApp::Application::"GitApp"
)
when { resource has contributors && principal in resource.contributors };
